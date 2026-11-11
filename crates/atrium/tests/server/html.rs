//! The output rules every page keeps, read with a real HTML parser.

use scraper::{Html, Selector};

fn select<'a>(page: &'a Html, selector: &str) -> Vec<scraper::ElementRef<'a>> {
    page.select(&Selector::parse(selector).unwrap()).collect()
}

/// Fails the test, naming `path`, where a page breaks a rule: inline script, `style`
/// attributes or elements, event-handler attributes, `javascript:` links, a script budget
/// over one deferred loader, no language, not exactly one `h1`, or no working skip link.
pub fn check(path: &str, body: &str) {
    let page = Html::parse_document(body);
    for element in page.root_element().descendent_elements() {
        let tag = element.value().name();
        assert_ne!(tag, "style", "{path}: a <style> element");
        for (name, value) in element.value().attrs() {
            assert_ne!(name, "style", "{path}: a style attribute on <{tag}>");
            assert!(
                !name.starts_with("on"),
                "{path}: an event handler {name} on <{tag}>"
            );
            assert!(
                !value
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:"),
                "{path}: {name}={value:?} on <{tag}>"
            );
        }
    }

    let scripts = select(&page, "script");
    let modules = select(&page, "[data-module]");
    let expected = usize::from(!modules.is_empty());
    assert_eq!(
        scripts.len(),
        expected,
        "{path}: one loader only with modules"
    );
    for script in scripts {
        assert_eq!(
            script.value().attr("type"),
            Some("module"),
            "{path}: deferred"
        );
        assert!(script.value().attr("src").is_some(), "{path}: external");
        assert_eq!(script.inner_html().trim(), "", "{path}: no inline script");
    }

    assert_eq!(
        select(&page, "html")[0].value().attr("lang"),
        Some("en"),
        "{path}: lang"
    );
    assert_eq!(select(&page, "h1").len(), 1, "{path}: one h1");
    let skip = select(&page, "body > a:first-child")[0];
    let target = skip.value().attr("href").unwrap().trim_start_matches('#');
    assert_eq!(
        select(&page, &format!("[id='{target}']")).len(),
        1,
        "{path}: skip link target"
    );
}

/// The text of every element `selector` picks, whitespace collapsed.
pub fn texts(body: &str, selector: &str) -> Vec<String> {
    let page = Html::parse_document(body);
    select(&page, selector)
        .into_iter()
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// The value of `attribute` on every element `selector` picks.
pub fn attrs(body: &str, selector: &str, attribute: &str) -> Vec<String> {
    let page = Html::parse_document(body);
    select(&page, selector)
        .into_iter()
        .filter_map(|element| element.value().attr(attribute).map(str::to_owned))
        .collect()
}

const GOOD: &str = r##"<!DOCTYPE html><html lang="en"><body><a href="#m">Skip</a><main id="m"><h1>T</h1>BODY</main></body></html>"##;

#[test]
fn the_check_passes_a_plain_page() {
    check("good", &GOOD.replace("BODY", "<p>text</p>"));
}

#[test]
#[should_panic(expected = "no inline script")]
fn the_check_sees_inline_script() {
    check(
        "bad",
        &GOOD.replace(
            "BODY",
            r#"<div data-module="x"></div><script type="module" src="/a.js">go()</script>"#,
        ),
    );
}

#[test]
#[should_panic(expected = "one loader only with modules")]
fn the_check_sees_a_script_on_a_page_without_modules() {
    check(
        "bad",
        &GOOD.replace("BODY", r#"<script type="module" src="/a.js"></script>"#),
    );
}

#[test]
#[should_panic(expected = "a style attribute")]
fn the_check_sees_style_attributes() {
    check(
        "bad",
        &GOOD.replace("BODY", r#"<p style="color:red">x</p>"#),
    );
}

#[test]
#[should_panic(expected = "an event handler")]
fn the_check_sees_event_handlers() {
    check("bad", &GOOD.replace("BODY", r#"<p onclick="x()">x</p>"#));
}

#[test]
#[should_panic(expected = "javascript:")]
fn the_check_sees_javascript_links() {
    check(
        "bad",
        &GOOD.replace("BODY", r#"<a href="javascript:x()">x</a>"#),
    );
}
