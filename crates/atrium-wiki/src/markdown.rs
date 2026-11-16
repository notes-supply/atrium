//! A page's Markdown as HTML, through comrak with GitHub's extensions and raw HTML off.
//! Links to other `.md` files lead inside the wiki, and links with a scheme other than
//! `http`, `https` or `mailto` become plain text. Relative links to other files and images
//! lead to Atrium's route for a file's bytes, when Mneme serves files; an image on another
//! site, which the page's policy would block, becomes a link to it.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use atrium_ui::components::{Mark, Want, mark};
use comrak::html::{Anchorizer, ChildRendering, escape, escape_href, format_node_default};
use comrak::nodes::{AstNode, ListType, NodeLink, NodeValue};
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

/// What Atrium knows of a file a page links to: its size and that it is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownFile {
    pub size: u64,
}

/// How a page's relative links to files and its images are written.
#[derive(Debug, Clone, Copy, Default)]
pub enum FileLinks<'a> {
    /// As the text has them, for a Mneme that serves no files.
    #[default]
    AsWritten,
    /// To Atrium's route for a file's bytes, at the commit `at` for an old version.
    Served {
        at: Option<&'a str>,
        /// The files Atrium knows of, by path, when it read a listing.
        known: Option<&'a Known>,
    },
}

/// The files a listing found, and whether it holds every file a page could link to, so
/// that one it lacks is not there.
#[derive(Debug, Default)]
pub struct Known {
    pub files: HashMap<String, KnownFile>,
    pub complete: bool,
}

/// The mark a link to a file carries: its kind, and its size when Atrium knows it.
#[derive(Debug, Clone, Default)]
struct Marks(HashMap<String, String>);

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
/// questions the wiki leaves for its readers.
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
create_formatter!(Article<Marks>, {
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
    NodeValue::Image(ref image) => |context, entering| {
        if entering {
            context.write_str("<img src=\"")?;
            escape_href(context, &image.url, false)?;
            context.write_str("\" alt=\"")?;
            return Ok(ChildRendering::Plain);
        }
        if !image.title.is_empty() {
            context.write_str("\" title=\"")?;
            escape(context, &image.title)?;
        }
        context.write_str("\" loading=\"lazy\" decoding=\"async\">")?;
    },
    NodeValue::Link(ref link) => |context, node, entering| {
        if let Some(mark) = context.user.0.get(&link.url).cloned() {
            if entering {
                context.write_str("<a class=\"file-link\" href=\"")?;
                escape_href(context, &link.url, false)?;
                context.write_str("\">")?;
            } else {
                context.write_str("<span class=\"file-mark\"><span class=\"vh\">, </span>")?;
                escape(context, &mark)?;
                context.write_str("</span></a>")?;
            }
            return Ok(ChildRendering::HTML);
        }
        if !link.url.starts_with("/wiki/") || link.url.starts_with("/wiki/raw/") {
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

/// The files a text links to or shows, by their paths in the wiki, as [`render`] resolves
/// them from `path`'s folder.
#[must_use]
pub fn files_linked(markdown: &str, path: &str) -> Vec<String> {
    if !formatted(markdown) {
        return Vec::new();
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options());
    let mut found: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for node in root.descendants() {
        let file = match &node.data().value {
            NodeValue::Link(link) => match target(&link.url, folder) {
                Target::File(file) => Some(file.path),
                _ => None,
            },
            NodeValue::Image(image) => match source(&image.url, folder) {
                Source::File(file) => Some(file.path),
                _ => None,
            },
            _ => None,
        };
        if let Some(file) = file
            && seen.insert(file.clone())
        {
            found.push(file);
        }
    }
    found
}

/// Renders `markdown`, the text of the page at `path`, whose relative links resolve from
/// `path`'s folder, and whose links to files and images are written as `files` says. The
/// first level-1 heading is the page's title, which the page shows in its own header, so it
/// is left out here.
#[must_use]
pub fn render(markdown: &str, path: &str, files: FileLinks<'_>) -> Rendered {
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
    let mut elsewhere = Vec::new();
    let mut marks = Marks::default();
    for node in root.descendants() {
        let mut data = node.data_mut();
        match &mut data.value {
            NodeValue::Link(link) => match target(&link.url, folder) {
                Target::Keep => {}
                Target::Wiki(url) => link.url = url,
                Target::File(file) => {
                    if let FileLinks::Served { at, known } = files {
                        link.url = file.href(at);
                        if let Some(mark) = mark_of(&file.path, at, known) {
                            marks.0.insert(link.url.clone(), mark);
                        }
                    }
                }
                Target::Unlink => unlinked.push(node),
            },
            NodeValue::Image(image) => match source(&image.url, folder) {
                Source::Keep => {}
                Source::File(file) => {
                    if let FileLinks::Served { at, .. } = files {
                        image.url = file.href(at);
                    }
                }
                Source::Elsewhere => elsewhere.push(node),
                Source::Drop => unlinked.push(node),
            },
            _ => {}
        }
    }
    for link in unlinked {
        unwrap(link);
    }
    for image in elsewhere {
        link_instead(&arena, image);
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
    if Article::format_document(root, &options, &mut out, marks).is_err() {
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

/// The mark a link to the file at `path` carries: its kind, and its size from `known`. A
/// file a whole listing lacks is not there, and its link carries no mark, as a link to a
/// page that is not there carries none either.
fn mark_of(path: &str, at: Option<&str>, known: Option<&Known>) -> Option<String> {
    let kind = crate::kind::label(path);
    let Some(known) = known.filter(|_| at.is_none()) else {
        return Some(kind);
    };
    match known.files.get(path) {
        Some(file) => Some(format!("{kind}, {}", atrium_ui::words::bytes(file.size))),
        None if known.complete => None,
        None => Some(kind),
    }
}

/// Turns an image on another site, which the page's policy would not load, into a link to
/// it, named by its alternative text and ending in "(image)". An image already inside a link,
/// such as a badge that links a build, becomes those words inside that link, so there is
/// never a link in a link and the link leads where it was written to.
fn link_instead<'a>(arena: &'a Arena<'a>, node: &'a AstNode<'a>) {
    let link = match &node.data().value {
        NodeValue::Image(image) => NodeLink::clone(image),
        _ => return,
    };
    let named = node.children().next().is_some();
    let words = if named {
        " (image)".to_owned()
    } else {
        format!("{} (image)", link.url)
    };
    node.append(arena.alloc(AstNode::from(NodeValue::Text(words.into()))));
    let in_a_link = node
        .ancestors()
        .skip(1)
        .any(|ancestor| matches!(ancestor.data().value, NodeValue::Link(_)));
    if in_a_link {
        unwrap(node);
    } else {
        node.data_mut().value = NodeValue::Link(Box::new(link));
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
    File(FileTarget),
    Unlink,
}

/// A file a link or an image names, by its path in the wiki.
#[derive(Debug, PartialEq, Eq)]
struct FileTarget {
    path: String,
    fragment: Option<String>,
}

impl FileTarget {
    /// Atrium's address for the file's bytes, at the commit `at` when one is given.
    fn href(&self, at: Option<&str>) -> String {
        let mut href = crate::kind::raw_href(&self.path, at);
        if let Some(fragment) = &self.fragment {
            href.push('#');
            href.push_str(fragment);
        }
        href
    }
}

/// What an image's source should load.
#[derive(Debug, PartialEq, Eq)]
enum Source {
    Keep,
    File(FileTarget),
    /// On another site, which the page's policy does not let load.
    Elsewhere,
    /// Something no image should load from, such as `data:` or `javascript:`.
    Drop,
}

/// A URL as a browser reads it: without tabs and newlines, and trimmed of spaces and controls.
fn seen(url: &str) -> String {
    let seen: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    seen.trim_matches(|c: char| c <= ' ').to_owned()
}

/// A path written in a page, resolved from `folder`, or from the top when it starts with a
/// slash: its segments, decoded, with `.` and `..` followed.
fn resolve(path: &str, folder: &str) -> Vec<String> {
    let start = if path.starts_with('/') { "" } else { folder };
    let mut segments: Vec<String> = Vec::new();
    for segment in start.split('/').map(str::to_owned).chain(
        path.split('/')
            .map(|segment| percent_decode_str(segment).decode_utf8_lossy().into_owned()),
    ) {
        match segment.as_str() {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    segments
}

/// The path and fragment of a relative URL.
fn parts(url: &str) -> (&str, Option<&str>) {
    let (rest, fragment) = url
        .split_once('#')
        .map_or((url, None), |(rest, fragment)| (rest, Some(fragment)));
    let path = rest.split_once('?').map_or(rest, |(path, _)| path);
    (path, fragment)
}

/// Where a link should lead. Browsers drop tabs and newlines from a URL and trim spaces and
/// controls around it before they read its scheme, so this does too. A relative link to a
/// file that is not a page leads to its bytes; one from the top of the site stays as
/// written, since it may name one of Atrium's own pages.
fn target(url: &str, folder: &str) -> Target {
    let seen = seen(url);
    if let Some(scheme) = scheme(&seen) {
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
    let (path, fragment) = parts(&seen);
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    if !is_page(&decoded) {
        let file = !path.is_empty() && !path.starts_with('/') && !path.ends_with('/');
        return match resolve(path, folder) {
            segments if file && !segments.is_empty() => Target::File(FileTarget {
                path: segments.join("/"),
                fragment: fragment.map(str::to_owned),
            }),
            _ => Target::Keep,
        };
    }
    let mut wiki = String::from("/wiki");
    for segment in resolve(path, folder) {
        wiki.push('/');
        wiki.extend(utf8_percent_encode(&segment, SEGMENT));
    }
    if let Some(fragment) = fragment {
        wiki.push('#');
        wiki.push_str(fragment);
    }
    Target::Wiki(wiki)
}

/// What an image's source should load: a file of the wiki, from the page's folder or from
/// the top, or an image on another site.
fn source(url: &str, folder: &str) -> Source {
    let seen = seen(url);
    if let Some(scheme) = scheme(&seen) {
        return if ["http", "https"]
            .iter()
            .any(|web| scheme.eq_ignore_ascii_case(web))
        {
            Source::Elsewhere
        } else {
            Source::Drop
        };
    }
    if seen.starts_with("//") {
        return Source::Elsewhere;
    }
    let (path, fragment) = parts(&seen);
    let segments = resolve(path, folder);
    if path.is_empty() || path.ends_with('/') || segments.is_empty() {
        return Source::Keep;
    }
    Source::File(FileTarget {
        path: segments.join("/"),
        fragment: fragment.map(str::to_owned),
    })
}

/// A path as an address: each segment escaped.
#[must_use]
pub fn path_href(path: &str) -> String {
    let mut href = String::with_capacity(path.len());
    for (i, segment) in path.split('/').enumerate() {
        if i > 0 {
            href.push('/');
        }
        href.extend(utf8_percent_encode(segment, SEGMENT));
    }
    href
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
        render(markdown, path, FileLinks::AsWritten)
            .html
            .into_string()
    }

    fn served(markdown: &str, path: &str, known: Option<&Known>) -> String {
        render(markdown, path, FileLinks::Served { at: None, known })
            .html
            .into_string()
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
    fn links_to_files_stay_for_a_mneme_that_serves_none() {
        let out = html("[a](#top) [b](picture.png) ![c](logo.svg)", "notes/a.md");
        assert!(out.contains(r##"href="#top""##), "{out}");
        assert!(out.contains(r#"href="picture.png""#), "{out}");
        assert!(out.contains(r#"<img src="logo.svg" alt="c""#), "{out}");
    }

    #[test]
    fn images_resolve_from_the_pages_folder_to_their_bytes() {
        let out = served(
            "![The logo](logo.svg \"Ours\") ![](../shared/a%20b.png) ![top](/brand/mark.png)",
            "brand/guide.md",
            None,
        );
        assert!(
            out.contains(r#"<img src="/wiki/raw/brand/logo.svg" alt="The logo" title="Ours" loading="lazy" decoding="async">"#),
            "{out}"
        );
        assert!(out.contains(r#"src="/wiki/raw/shared/a%20b.png""#), "{out}");
        assert!(out.contains(r#"src="/wiki/raw/brand/mark.png""#), "{out}");
        let old = render(
            "![x](logo.svg)",
            "brand/guide.md",
            FileLinks::Served {
                at: Some("faf510a1fb457f1af5496ca64dea77ed68ddb2f8"),
                known: None,
            },
        )
        .html
        .into_string();
        assert!(
            old.contains(
                r#"src="/wiki/raw/brand/logo.svg?at=faf510a1fb457f1af5496ca64dea77ed68ddb2f8""#
            ),
            "an old version shows its images as they were: {old}"
        );
    }

    #[test]
    fn an_image_on_another_site_is_a_link_to_it() {
        let out = served(
            "![A chart](https://example.com/c.png) ![](http://example.com/d.png) ![e](//example.com/e.png)",
            "a.md",
            None,
        );
        assert!(!out.contains("<img"), "{out}");
        assert!(
            out.contains(r#"<a href="https://example.com/c.png">A chart (image)</a>"#),
            "{out}"
        );
        assert!(
            out.contains(
                r#"<a href="http://example.com/d.png">http://example.com/d.png (image)</a>"#
            ),
            "{out}"
        );
        assert!(
            out.contains(r#"<a href="//example.com/e.png">e (image)</a>"#),
            "{out}"
        );
        let badge = served(
            "[![Status](https://example.com/badge.svg)](https://example.com/build) and [![](https://example.com/b.svg)](/wiki/changes)",
            "a.md",
            None,
        );
        assert!(
            badge.contains(r#"<a href="https://example.com/build">Status (image)</a>"#),
            "the link keeps its target, with the image's words in it: {badge}"
        );
        assert!(
            badge.contains(">https://example.com/b.svg (image)</a>"),
            "{badge}"
        );
        assert_eq!(
            badge.matches("<a ").count(),
            2,
            "no link in a link: {badge}"
        );
        let dropped = served(
            "![pixel](data:image/png;base64,AAAA) ![x](javascript:x)",
            "a.md",
            None,
        );
        assert!(
            !dropped.contains("<img") && !dropped.contains("<a"),
            "{dropped}"
        );
        assert!(dropped.contains("pixel"), "{dropped}");
    }

    #[test]
    fn a_link_to_a_file_carries_its_kind_and_the_size_a_listing_knows() {
        let mut known = Known {
            complete: true,
            ..Known::default()
        };
        known
            .files
            .insert("brand/kit.zip".to_owned(), KnownFile { size: 3000 });
        let out = served(
            "[The kit](kit.zip) and [gone](old.pdf) and [a page](other.md) and [home](/wiki/changes)",
            "brand/guide.md",
            Some(&known),
        );
        assert!(
            out.contains(r#"<a class="file-link" href="/wiki/raw/brand/kit.zip">The kit<span class="file-mark"><span class="vh">, </span>ZIP, 2.9 KiB</span></a>"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<a href="/wiki/raw/brand/old.pdf">gone</a>"#),
            "a file the whole listing lacks is a plain link: {out}"
        );
        assert!(
            out.contains(r#"data-preview="/wiki/brand/other.md""#),
            "{out}"
        );
        assert!(
            out.contains(r#"<a href="/wiki/changes" data-preview="/wiki/changes">home</a>"#),
            "a link from the top stays as written: {out}"
        );
        let partial = Known::default();
        let unknown = served("[x](old.pdf)", "a.md", Some(&partial));
        assert!(unknown.contains("PDF</span></a>"), "{unknown}");
    }

    #[test]
    fn the_files_a_text_links_are_listed_once_by_their_paths() {
        assert_eq!(
            files_linked(
                "![l](logo.svg) [k](kit.zip#x) [k](./kit.zip) [p](a.md) [w](https://e.com/z.zip) [t](/top.zip) ![x](../x.png)",
                "brand/guide.md"
            ),
            ["brand/logo.svg", "brand/kit.zip", "x.png"]
        );
    }

    #[test]
    fn the_title_is_left_out_and_the_headings_are_listed() {
        let rendered = render(
            "# Alpha\n\n## Restore a file\n\ntext\n\n## Known `issues`\n",
            "a.md",
            FileLinks::AsWritten,
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
