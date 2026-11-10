//! Render semantic paragraphs/BRs, rather than HTML source indentation.
use ego_tree::NodeRef;
use scraper::{ElementRef, Node};

fn block(tag: &str) -> bool {
    matches!(tag, "p" | "div" | "section" | "li" | "h2" | "h3" | "pre")
}
fn newline(output: &mut String) {
    while output.ends_with([' ', '\t', '\r']) {
        output.pop();
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
}
fn render(node: NodeRef<'_, Node>, output: &mut String, ruby: bool, pre: bool) {
    match node.value() {
        Node::Text(text) if pre => output.push_str(text),
        Node::Text(text) => {
            let formatting = text.chars().all(|ch| ch.is_ascii_whitespace())
                && node.parent().is_some_and(|parent| {
                    parent.children().any(|child| {
                        matches!(child.value(), Node::Element(element) if block(element.name()))
                    })
                });
            if formatting {
                return;
            }
            for ch in text.chars() {
                if ch.is_ascii_whitespace() {
                    if !output.is_empty() && !output.ends_with([' ', '\n']) {
                        output.push(' ');
                    }
                } else {
                    output.push(ch);
                }
            }
        }
        Node::Element(element) => {
            let tag = element.name();
            if matches!(tag, "script" | "style" | "noscript")
                || (!ruby && matches!(tag, "rt" | "rp"))
            {
                return;
            }
            if tag == "br" {
                while output.ends_with([' ', '\t', '\r']) {
                    output.pop();
                }
                output.push('\n');
                return;
            }
            if block(tag) {
                newline(output);
            }
            let start = output.len();
            for child in node.children() {
                render(child, output, ruby, pre || tag == "pre");
            }
            if block(tag) {
                if output.len() == start {
                    // An explicit empty paragraph is an authored blank line.
                    output.push('\n');
                } else {
                    newline(output);
                }
            }
        }
        _ => {}
    }
}
pub(super) fn lines(element: ElementRef<'_>, ruby: bool) -> Vec<String> {
    let mut text = String::new();
    render(*element, &mut text, ruby, false);
    let lines = text
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect::<Vec<_>>();
    let first = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(first, |index| index + 1);
    lines[first..end].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn extract(source: &str) -> Vec<String> {
        let document = scraper::Html::parse_document(source);
        let selector = scraper::Selector::parse("#body").unwrap();
        lines(document.select(&selector).next().unwrap(), false)
    }
    #[test]
    fn source_indentation_does_not_add_blank_rows_but_authored_pauses_remain() {
        assert_eq!(extract("<div id='body'>\n  <p>　「待って！」</p>\n\n  <p>彼は<ruby>扉<rt>とびら</rt></ruby>を開けた。</p>\n <p><br></p>\n <p>次の行<br>続き</p>\n</div>"), ["　「待って！」", "彼は扉を開けた。", "", "次の行", "続き"]);
    }
    #[test]
    fn inline_words_keep_required_spaces_and_preformatted_lines_remain_separate() {
        assert_eq!(extract("<div id='body'><p>Hello <b>world</b>\n again</p><pre>one\n  two</pre><p>終わり</p></div>"), ["Hello world again", "one", "  two", "終わり"]);
        assert_eq!(
            extract("<div id='body'><p>A</p><p></p><p></p><p>B</p></div>"),
            ["A", "", "", "B"]
        );
    }
}
