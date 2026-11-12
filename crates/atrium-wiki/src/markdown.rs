//! A page's Markdown as HTML, through comrak with GitHub's extensions and raw HTML off.
//! Links to other `.md` files lead inside the wiki, and links with a scheme other than
//! `http`, `https` or `mailto` become plain text.

use std::fmt::Write;

use atrium_ui::components::{Mark, Want, mark};
use comrak::html::{Anchorizer, ChildRendering, escape, escape_href, format_node_default};
use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Arena, Options, create_formatter};
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
    /// The headings below the title, in order, with the ids the HTML gives them.
    pub headings: Vec<Heading>,
}

/// A heading of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub id: String,
    pub text: String,
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
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

/// The words a node shows, as comrak reads a heading's.
fn text_of<'a>(node: &'a AstNode<'a>) -> String {
    node.descendants()
        .filter_map(|child| match &child.data().value {
            NodeValue::Text(text) => Some(text.to_string()),
            NodeValue::Code(code) => Some(code.literal.clone()),
            NodeValue::SoftBreak | NodeValue::LineBreak => Some(" ".to_owned()),
            _ => None,
        })
        .collect()
}

/// Whether a list is the one under a heading named "Open questions", which holds the
/// questions the wiki leaves for its owner.
fn open_questions<'a>(list: &'a AstNode<'a>) -> bool {
    let bullets =
        matches!(&list.data().value, NodeValue::List(list) if list.list_type == ListType::Bullet);
    bullets
        && list.previous_sibling().is_some_and(|heading| {
            matches!(heading.data().value, NodeValue::Heading(_))
                && text_of(heading)
                    .trim()
                    .eq_ignore_ascii_case("open questions")
        })
}

/// What a screen reader calls a table: the heading above it, or "Table".
fn region<'a>(table: &'a AstNode<'a>) -> String {
    table
        .preceding_siblings()
        .skip(1)
        .find(|node| matches!(node.data().value, NodeValue::Heading(_)))
        .map_or_else(|| "Table".to_owned(), text_of)
}

/// The header text of the column a body cell is in.
fn column_label<'a>(cell: &'a AstNode<'a>) -> (usize, String) {
    let column = cell.preceding_siblings().count().saturating_sub(1);
    let label = cell
        .parent()
        .and_then(AstNode::parent)
        .and_then(AstNode::first_child)
        .and_then(|head| head.children().nth(column))
        .map_or_else(String::new, text_of);
    (column, label)
}

fn attribute(text: &str) -> String {
    html! { (text) }.into_string().replace('"', "&quot;")
}

// The prototype's article markup: a heading carries its id, without comrak's permalink, which
// would be an invisible stop for the keyboard; a table scrolls in a focusable region whose rows are headed
// by their first cell and whose cells carry their column's name for the phone layout, code
// sits in a focusable figure, the open questions carry the mark that they want you, and a
// link to another page names it for the preview module.
create_formatter!(Article, {
    NodeValue::Heading(ref heading) => |context, node, entering| {
        if entering {
            let id = context.anchorizer.anchorize(&text_of(node));
            context.write_str(&format!("<h{} id=\"{}\">", heading.level, attribute(&id)))?;
        } else {
            context.write_str(&format!("</h{}>\n", heading.level))?;
        }
    },
    NodeValue::Table(_) => |context, node, entering| {
        if entering {
            context.write_str(&format!(
                r#"<div class="table-wrap" tabindex="0" role="region" aria-label="{}">"#,
                attribute(&region(node))
            ))?;
            return format_node_default(context, node, true);
        }
        format_node_default(context, node, false)?;
        context.write_str("</div>\n")?;
    },
    NodeValue::TableCell => |context, node, entering| {
        let header = node
            .parent()
            .is_some_and(|row| matches!(row.data().value, NodeValue::TableRow(true)));
        if header {
            return format_node_default(context, node, entering);
        }
        let (column, label) = column_label(node);
        let tag = if column == 0 { "th" } else { "td" };
        if entering {
            if column == 0 {
                context.write_str(r#"<th scope="row">"#)?;
            } else {
                context.write_str(&format!(r#"<td data-label="{}">"#, attribute(&label)))?;
            }
        } else {
            context.write_str(&format!("</{tag}>\n"))?;
        }
    },
    NodeValue::CodeBlock(ref block) => |context, entering| {
        if entering {
            let language = block.info.split_whitespace().next().unwrap_or_default();
            context.write_str(r#"<figure class="code"><pre tabindex="0" aria-label="Code"><code"#)?;
            if !language.is_empty() {
                context.write_str(&format!(r#" class="language-{}""#, attribute(language)))?;
            }
            context.write_str(">")?;
            escape(context, &block.literal)?;
            context.write_str("</code></pre></figure>\n")?;
        }
        return Ok(ChildRendering::Skip);
    },
    NodeValue::List(_) => |context, node, entering| {
        if !open_questions(node) {
            return format_node_default(context, node, entering);
        }
        context.write_str(if entering { "<ul class=\"open-questions\">\n" } else { "</ul>\n" })?;
    },
    NodeValue::Item(_) => |context, node, entering| {
        if !node.parent().is_some_and(open_questions) {
            return format_node_default(context, node, entering);
        }
        if entering {
            context.write_str("<li>")?;
            context.write_str(&mark(Mark::Want(Want::Needs)).into_string())?;
            context.write_str("<span>")?;
        } else {
            context.write_str("</span></li>\n")?;
        }
    },
    NodeValue::Link(ref link) => |context, node, entering| {
        if !link.url.starts_with("/wiki/") {
            return format_node_default(context, node, entering);
        }
        if entering {
            context.write_str("<a href=\"")?;
            escape_href(context, &link.url, false)?;
            context.write_str("\" data-preview=\"")?;
            escape_href(context, &link.url, false)?;
            context.write_str("\">")?;
        } else {
            context.write_str("</a>")?;
        }
    },
});

/// Renders `markdown`, the text of the page at `path`, whose relative links resolve from
/// `path`'s folder. The first level-1 heading is the page's title, which the page shows in
/// its own header, so it is left out here.
#[must_use]
pub fn render(markdown: &str, path: &str) -> Rendered {
    if !formatted(markdown) {
        return Rendered {
            html: html! { pre { code { (markdown) } } },
            headings: Vec::new(),
        };
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let arena = Arena::new();
    let options = options();
    let root = comrak::parse_document(&arena, markdown, &options);
    let mut unlinked = Vec::new();
    for node in root.descendants() {
        let mut data = node.data_mut();
        if let NodeValue::Link(link) = &mut data.value {
            match target(&link.url, folder) {
                Target::Keep => {}
                Target::Wiki(url) => link.url = url,
                Target::Unlink => unlinked.push(node),
            }
        }
    }
    for link in unlinked {
        unwrap(link);
    }
    if let Some(title) = root.children().find(
        |node| matches!(&node.data().value, NodeValue::Heading(heading) if heading.level == 1),
    ) {
        title.detach();
    }
    let mut anchors = Anchorizer::new();
    let headings = root
        .descendants()
        .filter_map(|node| {
            match &node.data().value {
                NodeValue::Heading(heading) => Some(heading.level),
                _ => None,
            }
            .map(|level| {
                let text = text_of(node);
                Heading {
                    level,
                    id: anchors.anchorize(&text),
                    text,
                }
            })
        })
        .collect();
    let mut out = String::with_capacity(markdown.len() * 2);
    if Article::format_document(root, &options, &mut out).is_err() {
        out.clear();
    }
    Rendered {
        html: PreEscaped(out),
        headings,
    }
}

/// The words of a text's first paragraph, as a preview shows them.
#[must_use]
pub fn first_paragraph(markdown: &str) -> String {
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options());
    root.descendants()
        .find(|node| matches!(node.data().value, NodeValue::Paragraph))
        .map_or_else(String::new, text_of)
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
    fn the_title_is_left_out_and_the_headings_are_listed() {
        let rendered = render(
            "# Alpha\n\n## Restore a file\n\ntext\n\n## Known `issues`\n",
            "a.md",
        );
        let out = rendered.html.into_string();
        assert!(!out.contains("<h1"), "{out}");
        assert_eq!(
            rendered
                .headings
                .iter()
                .map(|h| (h.id.as_str(), h.text.as_str()))
                .collect::<Vec<_>>(),
            [
                ("restore-a-file", "Restore a file"),
                ("known-issues", "Known issues")
            ]
        );
        assert!(
            out.contains(r#"<h2 id="restore-a-file">Restore a file</h2>"#),
            "{out}"
        );
    }

    #[test]
    fn a_preview_is_the_first_paragraphs_words() {
        assert_eq!(
            first_paragraph("# Title\n\nOnly [laurel](a.md) may *reach* `tank`.\n\nMore."),
            "Only laurel may reach tank."
        );
    }

    #[test]
    fn tables_scroll_in_a_region_with_labelled_cells() {
        let out = html(
            "## Schedule\n\n| Job | When |\n| - | - |\n| home | 03:00 |\n",
            "a.md",
        );
        assert!(
            out.contains(
                r#"<div class="table-wrap" tabindex="0" role="region" aria-label="Schedule">"#
            ),
            "{out}"
        );
        assert!(out.contains(r#"<th scope="row">home</th>"#), "{out}");
        assert!(out.contains(r#"<td data-label="When">03:00</td>"#), "{out}");
    }

    #[test]
    fn open_questions_carry_the_mark_and_code_sits_in_a_figure() {
        let out = html(
            "## Open questions\n\n- Weekly or nightly?\n\n```sh\nls <x>\n```\n",
            "a.md",
        );
        assert!(out.contains(r#"<ul class="open-questions">"#), "{out}");
        assert!(
            out.contains(r#"data-want="2"></svg><span>Weekly or nightly?</span></li>"#)
                || out.contains("<span>Weekly or nightly?</span></li>"),
            "{out}"
        );
        assert!(out.contains(r#"<figure class="code"><pre tabindex="0" aria-label="Code"><code class="language-sh">ls &lt;x&gt;"#), "{out}");
    }

    #[test]
    fn a_link_to_another_page_names_it_for_previews() {
        let out = html("[ACLs](acl.md)", "homelab/a.md");
        assert!(
            out.contains(
                r#"<a href="/wiki/homelab/acl.md" data-preview="/wiki/homelab/acl.md">ACLs</a>"#
            ),
            "{out}"
        );
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
