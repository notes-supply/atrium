//! The page shell around every feature's pages, the small components features share, and the
//! stylesheet. Plain semantic HTML, so the chosen design can replace it.

use std::future::{Future, ready};
use std::sync::Arc;

use atrium_core::{Command, NavEntry};
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use jiff::Timestamp;
use maud::{DOCTYPE, Markup, Render, html};

/// The whole stylesheet. The server serves it under a name with its hash in it.
pub const STYLESHEET: &str = include_str!("atrium.css");

/// The id of the element the skip link leads to. Markdown headings get ids too, so it is
/// unlikely to be a heading's.
const MAIN: &str = "main-content";

/// What every page around a feature's content shows, for the viewer a listener serves.
#[derive(Debug, Clone)]
pub struct Chrome {
    /// The site navigation, in registry order.
    pub nav: Vec<NavEntry>,
    /// Every command the palette offers, which each page also lists as links.
    pub commands: Vec<Command>,
    /// The stylesheet's URL.
    pub stylesheet: String,
    /// The module loader's URL: the page's one script, when it has a module.
    pub loader: String,
    /// The command palette's module.
    pub palette: Module,
}

/// A Wasm module's files, as the loader reads them from the element that names the module.
#[derive(Debug, Clone)]
pub struct Module {
    /// The URL of its JavaScript glue.
    pub glue: String,
    /// The URL of its `.wasm`.
    pub wasm: String,
}

/// The shell for one request: the chrome and the path, so the current navigation entry is
/// marked. A handler takes it as an argument; the server puts the chrome in each request.
#[derive(Debug, Clone)]
pub struct Shell {
    chrome: Arc<Chrome>,
    path: String,
}

impl<S: Send + Sync> FromRequestParts<S> for Shell {
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let shell = parts
            .extensions
            .get::<Arc<Chrome>>()
            .map(|chrome| Self::new(Arc::clone(chrome), parts.uri.path()))
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);
        ready(shell)
    }
}

impl Shell {
    /// A shell for `path` without a request, for tests and for middleware.
    #[must_use]
    pub fn new(chrome: Arc<Chrome>, path: &str) -> Self {
        Self {
            chrome,
            path: path.to_owned(),
        }
    }

    /// A 200 page titled `title` around `main`.
    #[must_use]
    pub fn page(&self, title: &str, main: impl Render) -> Response {
        self.respond(StatusCode::OK, title, main)
    }

    /// A page with `status`, titled `title`, around `main`.
    #[must_use]
    pub fn respond(&self, status: StatusCode, title: &str, main: impl Render) -> Response {
        (status, self.document(title, &main)).into_response()
    }

    /// The page for an address with nothing at it, or nothing this viewer may see.
    #[must_use]
    pub fn not_found(&self) -> Response {
        self.respond(
            StatusCode::NOT_FOUND,
            "Not found",
            html! {
                h1 { "Not found" }
                p { "There is nothing at this address." }
            },
        )
    }

    fn current(&self, href: &str) -> bool {
        match href {
            "/" => self.path == "/",
            _ => self
                .path
                .strip_prefix(href)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
        }
    }

    fn document(&self, title: &str, main: &impl Render) -> Markup {
        let chrome = &self.chrome;
        // The palette is the only module so far; a page loads the loader only with one.
        let palette = !chrome.commands.is_empty();
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (title) " · Atrium" }
                    link rel="stylesheet" href=(chrome.stylesheet);
                    @if palette {
                        script type="module" src=(chrome.loader) {}
                    }
                }
                body {
                    a.skip href={ "#" (MAIN) } { "Skip to content" }
                    header {
                        nav aria-label="Site" {
                            ul {
                                @for entry in &chrome.nav {
                                    li {
                                        a href=(entry.href)
                                            aria-current=[self.current(entry.href).then_some("page")]
                                        { (entry.label) }
                                    }
                                }
                            }
                        }
                    }
                    main id=(MAIN) { (main) }
                    @if palette {
                        footer {
                            nav aria-labelledby="commands" data-module="palette"
                                data-glue=(chrome.palette.glue) data-wasm=(chrome.palette.wasm)
                            {
                                h2 #commands { "Commands" }
                                p { "Ctrl+K or ⌘K opens these anywhere." }
                                ul {
                                    @for command in &chrome.commands {
                                        li {
                                            a href=(command.href) data-keys=[command.keys] {
                                                (command.label)
                                            }
                                            @if let Some(keys) = command.keys {
                                                " " kbd { (keys) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A time in seconds since the Unix epoch, as a `<time>` element in UTC.
#[must_use]
pub fn time(seconds: i64) -> Markup {
    Timestamp::from_second(seconds).map_or_else(
        |_| html! { (seconds) },
        |at| html! { time datetime=(at) { (at.strftime("%Y-%m-%d %H:%M")) } },
    )
}

/// The day of a time in seconds since the Unix epoch, in UTC, as `2026-09-01`.
#[must_use]
pub fn day(seconds: i64) -> String {
    Timestamp::from_second(seconds).map_or_else(
        |_| seconds.to_string(),
        |at| at.strftime("%Y-%m-%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(path: &str) -> Shell {
        Shell::new(
            Arc::new(Chrome {
                nav: vec![
                    NavEntry {
                        label: "Home",
                        href: "/",
                    },
                    NavEntry {
                        label: "Wiki",
                        href: "/wiki",
                    },
                ],
                commands: Vec::new(),
                stylesheet: "/assets/atrium.css".to_owned(),
                loader: "/assets/loader.js".to_owned(),
                palette: Module {
                    glue: "/assets/palette.js".to_owned(),
                    wasm: "/assets/palette_bg.wasm".to_owned(),
                },
            }),
            path,
        )
    }

    #[test]
    fn marks_the_entry_whose_section_holds_the_path() {
        let page = shell("/wiki/page/0123456789abcdef")
            .document("t", &html! {})
            .into_string();
        assert!(page.contains(r#"<a href="/wiki" aria-current="page">"#));
        assert!(page.contains(r#"<a href="/">"#));
    }

    #[test]
    fn home_is_current_only_at_the_root() {
        let page = shell("/").document("t", &html! {}).into_string();
        assert!(page.contains(r#"<a href="/" aria-current="page">"#));
        assert!(!shell("/wikipedia").current("/wiki"));
    }

    #[test]
    fn times_are_utc() {
        assert_eq!(
            time(1_788_220_800).into_string(),
            r#"<time datetime="2026-09-01T00:00:00Z">2026-09-01 00:00</time>"#
        );
        assert_eq!(day(1_788_307_199), "2026-09-01");
    }
}
