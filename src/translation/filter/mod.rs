//! Literal protection and compatibility preprocessing, separate from NMT.
mod dictionary;
mod particles;
mod text;
use crate::translation::engine::Translator;
use aho_corasick::{AhoCorasickBuilder, MatchKind};
use anyhow::Result;
pub use dictionary::{Dictionary, DictionaryEntry};
use particles::resolve_token;
use text::{is_emoji, is_japanese};

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub dictionary: Dictionary,
    strip_engine_brackets: bool,
    natural_text: bool,
    memory: Option<crate::learning::Memory>,
}
impl Pipeline {
    pub fn new(dictionary: Dictionary) -> Self {
        Self {
            dictionary,
            strip_engine_brackets: false,
            natural_text: false,
            memory: None,
        }
    }
    pub fn legacy(dictionary: Dictionary) -> Self {
        Self {
            dictionary,
            strip_engine_brackets: true,
            natural_text: false,
            memory: None,
        }
    }
    pub fn translate(&self, engine: &mut dyn Translator, input: &str) -> Result<String> {
        let protected = self.dictionary.matcher.as_ref().is_some_and(|matcher| {
            matcher.is_match(input) || matcher.is_match(&text::normalize_punctuation(input))
        });
        if !protected {
            if let Some(corrected) = self
                .memory
                .as_ref()
                .and_then(|memory| memory.resolve(input))
            {
                return Ok(corrected.into_owned());
            }
        }
        let natural = self.natural_text && !protected;
        let prepared = if natural {
            super::prose::japanese::expand(input)
        } else {
            std::borrow::Cow::Borrowed(input)
        };
        let protected = protected
            || self.dictionary.matcher.as_ref().is_some_and(|matcher| {
                matcher.is_match(prepared.as_ref())
                    || matcher.is_match(&text::normalize_punctuation(&prepared))
            });
        let translated = self.translate_inner(engine, &prepared, 0)?;
        // Explicit dictionary choices retain their exact spelling and spacing.
        // Normalize only after placeholders/brackets have been restored.
        let translated = if natural && !protected {
            super::prose::finish_lines(input, &translated)
        } else {
            translated
        };
        if !protected && !input.contains('\n') {
            if let Some(memory) = &self.memory {
                return memory.post_edit(input, &translated);
            }
        }
        Ok(translated)
    }
    pub fn with_natural_text(mut self) -> Self {
        self.natural_text = true;
        self
    }
    /// Exact whole-utterance corrections precede inference; source-qualified
    /// terms follow it. Explicit dictionary entries retain priority. Term edits
    /// require a single source row rather than guessing a paragraph alignment.
    pub fn with_memory(mut self, memory: crate::learning::Memory) -> Self {
        self.memory = Some(memory);
        self
    }
    fn translate_inner(
        &self,
        engine: &mut dyn Translator,
        input: &str,
        depth: u8,
    ) -> Result<String> {
        if input.is_empty() || !is_japanese(input) {
            return Ok(input.to_owned());
        }
        if depth > 12 {
            return engine.translate(input);
        }
        let mut special_prefix = "s$".to_owned();
        while input.contains(&special_prefix) {
            special_prefix.push('_');
        }
        let mut dictionary_prefix = "c$".to_owned();
        while input.contains(&dictionary_prefix) {
            dictionary_prefix.push('_');
        }
        let mut protected = Vec::<(String, String, bool)>::new();
        let mut work = String::with_capacity(input.len());
        // translator7 normalizes full-width punctuation before BracketFilter.
        let normalized = text::normalize_punctuation(input);
        let mut chars = normalized.char_indices().peekable();
        while let Some((_, c)) = chars.next() {
            match c {
                '(' | '[' => {
                    let close = if c == '(' { ')' } else { ']' };
                    let mut part = String::new();
                    let mut found = false;
                    while let Some((_, next)) = chars.peek().copied() {
                        if next == c {
                            break;
                        }
                        chars.next();
                        if next == close {
                            found = true;
                            break;
                        }
                        part.push(next);
                    }
                    if found {
                        let inner = self.translate_inner(engine, &part, depth + 1)?;
                        let token = format!("{special_prefix}{}$s", protected.len());
                        work.push_str(&token);
                        protected.push((token, format!("{c}{inner}{close}"), false));
                    } else {
                        work.push(c);
                        work.push_str(&part);
                    }
                }
                c if is_emoji(c) => {
                    let token = format!("{special_prefix}{}$s", protected.len());
                    work.push_str(&token);
                    protected.push((token, c.to_string(), false));
                }
                _ => work.push(c),
            }
        }
        if let Some(matcher) = &self.dictionary.matcher {
            let mut output = String::with_capacity(work.len());
            let mut end = 0;
            for hit in matcher.find_iter(&work) {
                output.push_str(&work[end..hit.start()]);
                let item = &self.dictionary.entries[hit.pattern().as_usize()];
                let token = format!("{dictionary_prefix}{}$c", protected.len());
                output.push_str(&token);
                if item.name {
                    output.push_str("#N");
                }
                protected.push((token, item.target.clone(), true));
                end = hit.end();
            }
            output.push_str(&work[end..]);
            work = output;
        }
        let mut translated = engine.translate(&work)?;
        if self.strip_engine_brackets {
            let pattern = regex::Regex::new(r"\[[^\[\]]*\]|\([^()]*\)")?;
            loop {
                let stripped = pattern.replace_all(&translated, "");
                if stripped.len() == translated.len() {
                    break;
                }
                translated = stripped.into_owned();
            }
        }
        if protected.is_empty() {
            return Ok(translated.trim_end().to_owned());
        }
        let tokens: Vec<&str> = protected.iter().map(|x| x.0.as_str()).collect();
        let matcher = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&tokens)?;
        let mut output = String::new();
        let mut end = 0;
        for hit in matcher.find_iter(&translated) {
            output.push_str(&translated[end..hit.start()]);
            let (_, replacement, dictionary) = &protected[hit.pattern().as_usize()];
            let tail = &translated[hit.end()..];
            let (text, skip) = if *dictionary {
                resolve_token(replacement, tail)
            } else {
                (replacement.to_owned(), 0)
            };
            output.push_str(&text);
            end = hit.end() + skip;
        }
        output.push_str(&translated[end..]);
        Ok(output.trim_end().to_owned())
    }
}

#[cfg(test)]
mod tests;
