//! A page's Markdown as HTML, through comrak with GitHub's extensions and raw HTML off.
//! Links to other `.md` files lead inside the wiki, and links with a scheme other than
//! `http`, `https` or `mailto` become plain text.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options};
use maud::{Markup, PreEscaped, html};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

/// Markup characters, lines, entities and autolinks (counted twice), as Mneme counts them.
/// Dense markup costs comrak about 600 bytes per marker, so one tree stays near 10 MB. A page
/// over the budget is shown as its raw text, which is also how Mneme treats it.
const MARKUP_BUDGET: usize = 16_384;

/// What a path segment must escape inside a URL path.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// A page's text, ready for the page.
#[derive(Debug)]
pub struct Rendered {
    pub html: Markup,
    /// Whether the text has its own level-1 heading, so the page needs no other.
    pub titled: bool,
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.header_id_prefix = Some(String::new());
    options
}

fn formatted(markdown: &str) -> bool {
    let markers = markdown
        .bytes()
        .filter(|b| b"*_`[]<>!\\|~#&@\n\r".contains(b))
        .count();
    let autolinks = markdown.matches("www.").count() + markdown.matches("://").count();
    markers + 2 * autolinks <= MARKUP_BUDGET
}

/// Renders `markdown`, the text of the page at `path`, whose relative links resolve from
/// `path`'s folder.
#[must_use]
pub fn render(markdown: &str, path: &str) -> Rendered {
    if !formatted(markdown) {
        return Rendered {
            html: html! { pre { code { (markdown) } } },
            titled: false,
        };
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let arena = Arena::new();
    let options = options();
    let root = comrak::parse_document(&arena, markdown, &options);
    let mut titled = false;
    let mut unlinked = Vec::new();
    for node in root.descendants() {
        let mut data = node.data_mut();
        match &mut data.value {
            NodeValue::Heading(heading) if heading.level == 1 => titled = true,
            NodeValue::Link(link) => match target(&link.url, folder) {
                Target::Keep => {}
                Target::Wiki(url) => link.url = url,
                Target::Unlink => unlinked.push(node),
            },
            _ => {}
        }
    }
    for link in unlinked {
        unwrap(link);
    }
    let mut out = String::with_capacity(markdown.len() * 2);
    if comrak::format_html(root, &options, &mut out).is_err() {
        out.clear();
    }
    Rendered {
        html: PreEscaped(out),
        titled,
    }
}

/// Puts a node's children where it was, and drops the node.
fn unwrap<'a>(node: &'a AstNode<'a>) {
    while let Some(child) = node.first_child() {
        node.insert_before(child);
    }
    node.detach();
}

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Keep,
    Wiki(String),
    Unlink,
}

/// Where a link should lead. Browsers drop tabs and newlines from a URL and trim spaces and
/// controls around it before they read its scheme, so this does too.
fn target(url: &str, folder: &str) -> Target {
    let seen: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let seen = seen.trim_matches(|c: char| c <= ' ');
    if let Some(scheme) = scheme(seen) {
        return if ["http", "https", "mailto"]
            .iter()
            .any(|safe| scheme.eq_ignore_ascii_case(safe))
        {
            Target::Keep
        } else {
            Target::Unlink
        };
    }
    if seen.starts_with("//") {
        return Target::Keep;
    }
    let (rest, fragment) = seen
        .split_once('#')
        .map_or((seen, None), |(rest, fragment)| (rest, Some(fragment)));
    let path = rest.split_once('?').map_or(rest, |(path, _)| path);
    let path = percent_decode_str(path).decode_utf8_lossy();
    if !is_page(&path) {
        return Target::Keep;
    }
    let start = if path.starts_with('/') { "" } else { folder };
    let mut segments: Vec<&str> = Vec::new();
    for segment in start.split('/').chain(path.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    let mut wiki = String::from("/wiki");
    for segment in segments {
        wiki.push('/');
        wiki.extend(utf8_percent_encode(segment, SEGMENT));
    }
    if let Some(fragment) = fragment {
        wiki.push('#');
        wiki.push_str(fragment);
    }
    Target::Wiki(wiki)
}

/// Whether `path` names a page: a Markdown file, by its extension.
pub fn is_page(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("md"))
}

/// The scheme of an absolute URL, as RFC 3986 spells one.
fn scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    let mut bytes = scheme.bytes();
    let starts = bytes.next().is_some_and(|b| b.is_ascii_alphabetic());
    let rest = bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'));
    (starts && rest).then_some(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(markdown: &str, path: &str) -> String {
        render(markdown, path).html.into_string()
    }

    #[test]
    fn raw_html_is_not_rendered() {
        let out = html(
            "<script>alert(1)</script>\n\nHi <b onclick=\"x()\">there</b>",
            "a.md",
        );
        assert!(!out.contains("<script"), "{out}");
        assert!(!out.contains("<b"), "{out}");
        assert!(!out.contains("onclick"), "{out}");
    }

    #[test]
    fn a_javascript_link_is_not_a_link() {
        for url in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            " javascript:alert(1)",
            "java&#9;script:alert(1)",
            "data:text/html,x",
            "vbscript:x",
        ] {
            let out = html(&format!("[click me]({url}) and <{url}>"), "a.md");
            assert!(!out.contains("<a"), "{url}: {out}");
            assert!(out.contains("click me"), "{url}: the text stays: {out}");
        }
    }

    #[test]
    fn web_and_mail_links_stay() {
        let out = html(
            "[w](https://example.com/x) [m](mailto:me@example.com)",
            "a.md",
        );
        assert!(out.contains(r#"href="https://example.com/x""#), "{out}");
        assert!(out.contains(r#"href="mailto:me@example.com""#), "{out}");
    }

    #[test]
    fn relative_md_links_resolve_inside_the_wiki() {
        let cases = [
            ("other.md", "notes/deep.md", "/wiki/notes/other.md"),
            ("../index.md", "notes/deep.md", "/wiki/index.md"),
            (
                "./sub/x.md#part",
                "notes/deep.md",
                "/wiki/notes/sub/x.md#part",
            ),
            ("/greek/alpha.md", "notes/deep.md", "/wiki/greek/alpha.md"),
            ("../../../alpha.md", "a.md", "/wiki/alpha.md"),
            ("AGENTS.MD", "index.md", "/wiki/AGENTS.MD"),
            (
                "minnen%20fr%C3%A5n%20gotland.md",
                "index.md",
                "/wiki/minnen%20fr%C3%A5n%20gotland.md",
            ),
            (
                "<minnen från gotland.md>",
                "index.md",
                "/wiki/minnen%20fr%C3%A5n%20gotland.md",
            ),
        ];
        for (link, page, expected) in cases {
            let out = html(&format!("[x]({link})"), page);
            assert!(
                out.contains(&format!(r#"href="{expected}""#)),
                "{link} from {page}: {out}"
            );
        }
    }

    #[test]
    fn other_relative_links_stay() {
        let out = html("[a](#top) [b](picture.png)", "notes/a.md");
        assert!(out.contains(r##"href="#top""##), "{out}");
        assert!(out.contains(r#"href="picture.png""#), "{out}");
    }

    #[test]
    fn a_heading_of_level_one_titles_the_page() {
        assert!(render("# Alpha\n\ntext", "a.md").titled);
        assert!(!render("## Alpha\n\ntext", "a.md").titled);
    }

    #[test]
    fn gfm_tables_and_strikethrough_render() {
        let out = html("| a | b |\n| - | - |\n| 1 | 2 |\n\n~~gone~~", "a.md");
        assert!(out.contains("<table>"), "{out}");
        assert!(out.contains("<del>gone</del>"), "{out}");
    }

    #[test]
    fn a_page_over_the_markup_budget_is_its_raw_text() {
        let dense = "*a* ".repeat(MARKUP_BUDGET);
        let out = html(&dense, "a.md");
        assert!(out.starts_with("<pre><code>*a* *a*"), "{}", &out[..40]);
    }
}
