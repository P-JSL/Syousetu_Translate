use anyhow::Result;
use ego_tree::NodeRef;
use scraper::{ElementRef, Html, Node};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
mod body;
thread_local! {
    static SELECTORS: RefCell<HashMap<String,Rc<scraper::Selector>>> = RefCell::default();
}
/// Internal queries are fixed by the site adapters, never supplied by HTML.
pub(super) fn select(query: &str) -> Result<Rc<scraper::Selector>, ()> {
    SELECTORS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(selector) = cache.get(query) {
            return Ok(Rc::clone(selector));
        }
        let selector = Rc::new(scraper::Selector::parse(query).map_err(|_| ())?);
        cache.insert(query.to_owned(), Rc::clone(&selector));
        Ok(selector)
    })
}
pub(super) fn first_text(doc: &Html, selectors: &[&str]) -> Option<String> {
    selectors.iter().find_map(|s| {
        let sel = select(s).ok()?;
        doc.select(&sel).find_map(|el| {
            el.value()
                .attr("content")
                .map(clean)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    let value = clean(&el.text().collect::<String>());
                    (!value.is_empty()).then_some(value)
                })
        })
    })
}
pub(super) fn clean(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(super) fn extract_text(node: NodeRef<'_, Node>, out: &mut String, include_furigana: bool) {
    match node.value() {
        Node::Text(x) => out.push_str(x),
        Node::Element(e) => {
            let tag = e.name();
            if matches!(tag, "script" | "style" | "noscript")
                || (!include_furigana && matches!(tag, "rp" | "rt"))
            {
                return;
            }
            if tag == "br" {
                out.push('\n');
                return;
            }
            for child in node.children() {
                extract_text(child, out, include_furigana);
            }
            if matches!(tag, "p" | "div" | "section" | "li" | "h2" | "h3") {
                out.push('\n');
            }
        }
        _ => {
            for child in node.children() {
                extract_text(child, out, include_furigana);
            }
        }
    }
}
pub(super) fn first_element<'a>(document: &'a Html, selectors: &[&str]) -> Option<ElementRef<'a>> {
    selectors.iter().find_map(|query| {
        let selector = select(query).ok()?;
        document
            .select(&selector)
            .find(|element| !clean(&element.text().collect::<String>()).is_empty())
    })
}
pub(super) fn element_lines(element: ElementRef<'_>, include_furigana: bool) -> Vec<String> {
    body::lines(element, include_furigana)
}
