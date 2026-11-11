//! The page shell around every feature's pages, the components the pages share, the drawings
//! and the stylesheet. Each piece writes the markup of the chosen design's prototype, so the
//! stylesheet and the browser modules find what they expect.

pub mod assets;
pub mod components;
pub mod draw;
pub mod words;

use std::fmt;
use std::future::{Future, ready};
use std::sync::Arc;

use atrium_core::{HomeOffer, Intensity, Local, NavEntry, Prefs, Search, Visit};
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::components::cmd_line;
use crate::draw::glyph;

/// The id of the element the skip link leads to.
pub const MAIN: &str = "main";

/// Writes a footer's line for the reader's clock.
pub type FootnoteFn = dyn Fn(&Local) -> Option<Markup> + Send + Sync;

/// Every page footer's line, from what the features already know.
#[derive(Clone)]
pub struct Footnote(pub Arc<FootnoteFn>);

impl fmt::Debug for Footnote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Footnote")
    }
}

/// What every page around a feature's content shows, for the viewer a listener serves.
#[derive(Debug, Clone)]
pub struct Chrome {
    /// The site navigation, in registry order. The first entry is key `1`.
    pub nav: Vec<NavEntry>,
    /// Where search forms send their words and the palette finds its pages.
    pub search: Option<Search>,
    /// The stylesheet's URL.
    pub stylesheet: String,
    /// The fonts the first paint needs.
    pub fonts: Vec<String>,
    /// The page's one script and the bundles it starts.
    pub scripts: Scripts,
    pub footnote: Footnote,
}

/// The module loader and the two Wasm bundles it starts, which its own `<script>` names.
#[derive(Debug, Clone, Default)]
pub struct Scripts {
    /// The loader's URL: the page's one script.
    pub loader: String,
    /// Every small behaviour.
    pub ui: Bundle,
    /// The WebGL background.
    pub field: Bundle,
}

/// A Wasm bundle's files.
#[derive(Debug, Clone, Default)]
pub struct Bundle {
    /// The URL of its JavaScript glue.
    pub glue: String,
    /// The URL of its `.wasm`.
    pub wasm: String,
}

/// Which navigation entry a page belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Current {
    /// The entry whose address is the longest start of the page's path.
    Path,
    /// The entry at this address.
    Href(&'static str),
    /// None, as on a page that is not there.
    Nothing,
}

/// One page: what goes in its head, between the masthead and the footer, and in the footer.
#[derive(Debug)]
pub struct Page {
    /// The whole `<title>`.
    pub title: String,
    pub description: &'static str,
    pub current: Current,
    /// Everything between the masthead and the footer, its `<main>` included.
    pub body: Markup,
    /// The footer's line, when the page says something of its own there.
    pub foot: Option<Markup>,
    /// Whether the page marks what is new since the last visit.
    pub since: bool,
}

impl Page {
    /// A page that belongs to the entry its path names, with the usual footer.
    #[must_use]
    pub fn new(title: impl Into<String>, description: &'static str, body: Markup) -> Self {
        Self {
            title: title.into(),
            description,
            current: Current::Path,
            body,
            foot: None,
            since: false,
        }
    }
}

/// The shell for one request: the chrome, the path, and the visit. A handler takes it as an
/// argument; the server puts the chrome and the visit in each request.
#[derive(Debug, Clone)]
pub struct Shell {
    chrome: Arc<Chrome>,
    path: String,
    visit: Visit,
}

impl<S: Send + Sync> FromRequestParts<S> for Shell {
    type Rejection = StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let chrome = parts.extensions.get::<Arc<Chrome>>();
        let visit = parts.extensions.get::<Visit>();
        let shell = chrome
            .zip(visit)
            .map(|(chrome, visit)| Self::new(Arc::clone(chrome), parts.uri.path(), visit.clone()))
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR);
        ready(shell)
    }
}

impl Shell {
    /// A shell for `path` without a request, for tests and for middleware.
    #[must_use]
    pub fn new(chrome: Arc<Chrome>, path: &str, visit: Visit) -> Self {
        Self {
            chrome,
            path: path.to_owned(),
            visit,
        }
    }

    /// The reader's clock.
    #[must_use]
    pub const fn local(&self) -> &Local {
        &self.visit.local
    }

    /// The reader's visit.
    #[must_use]
    pub const fn visit(&self) -> &Visit {
        &self.visit
    }

    /// The address asked for.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Where search forms send their words.
    #[must_use]
    pub fn search(&self) -> &'static str {
        self.chrome.search.map_or("/", |search| search.action)
    }

    /// Where the palette finds its pages.
    #[must_use]
    pub fn pages(&self) -> Option<&'static str> {
        self.chrome.search.map(|search| search.pages)
    }

    /// A 200 page.
    #[must_use]
    pub fn page(&self, page: Page) -> Response {
        self.respond(StatusCode::OK, page)
    }

    /// A page with `status`.
    #[must_use]
    pub fn respond(&self, status: StatusCode, page: Page) -> Response {
        (status, self.document(page)).into_response()
    }

    /// The page for an address with nothing at it, or nothing this viewer may see. `gone`
    /// says what the record remembers of a page that was there, and `ways` are links on.
    #[must_use]
    pub fn lost(&self, gone: Option<Markup>, ways: &[(String, String)]) -> Response {
        let word = self
            .path
            .rsplit('/')
            .find(|part| !part.is_empty())
            .map(|part| part.trim_end_matches(".md").replace(['-', '_'], " "));
        let body = html! {
            main #(MAIN) .not-found {
                section.lost aria-labelledby="lost-h" {
                    p.code-404 { "404, " (self.path) }
                    h1 #lost-h { "Nothing at this address" }
                    @if let Some(gone) = gone {
                        p { "There is no page here now. The record remembers one that was:" }
                        div.gone.surface data-surface { (gone) }
                    } @else {
                        p { "There is no page here." }
                    }
                    @if let Some(search) = self.chrome.search {
                        (cmd_line("lost", search.action, Some(search.pages)))
                    }
                }
                aside.lost-side.surface data-surface aria-labelledby="ways-h" {
                    h2 #ways-h .rail-h { "Ways on" }
                    a.more href="/" { "Home" }
                    @if let (Some(search), Some(word)) = (self.chrome.search.map(|search| search.action), &word) {
                        a.more href=(format!("{search}?q={}", words::query(word))) { "Search for “" (word) "”" }
                    }
                    @for (label, href) in ways {
                        a.more href=(href) { (label) }
                    }
                }
            }
        };
        let mut page = Page::new("Not found, Atrium", "Nothing at this address.", body);
        page.current = Current::Nothing;
        self.respond(StatusCode::NOT_FOUND, page)
    }

    /// The plain page for an address with nothing at it.
    #[must_use]
    pub fn not_found(&self) -> Response {
        self.lost(None, &[])
    }

    fn current(&self, current: Current) -> Option<&'static str> {
        match current {
            Current::Nothing => None,
            Current::Href(href) => Some(href),
            Current::Path => self
                .chrome
                .nav
                .iter()
                .map(|entry| entry.href)
                .filter(|href| match *href {
                    "/" => self.path == "/",
                    href => self
                        .path
                        .strip_prefix(href)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/')),
                })
                .max_by_key(|href| href.len()),
        }
    }

    fn document(&self, page: Page) -> Markup {
        let chrome = &self.chrome;
        let local = &self.visit.local;
        let prefs = &self.visit.prefs;
        let current = self.current(page.current);
        let body = page.body.into_string();
        // The drawings of places fill their faces from patterns every page defines once.
        let places = body.contains(r#"<g class="m""#);
        let modules = if page.since {
            "keys light carry since"
        } else {
            "keys light carry"
        };
        html! {
            (DOCTYPE)
            html lang="en" data-hour=(local.hour()) data-intensity=(prefs.intensity.name())
                data-theme=[prefs.theme.map(atrium_core::Theme::name)]
                data-keys=[(!prefs.letter_keys).then_some("off")]
                data-entrance=(self.visit.entrance.name())
            {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover";
                    meta name="color-scheme" content="light dark";
                    title { (page.title) }
                    meta name="description" content=(page.description);
                    @for font in &chrome.fonts {
                        link rel="preload" href=(font) as="font" type="font/woff2" crossorigin;
                    }
                    link rel="stylesheet" href=(chrome.stylesheet);
                    script type="module" src=(chrome.scripts.loader)
                        data-ui=(chrome.scripts.ui.glue) data-ui-wasm=(chrome.scripts.ui.wasm)
                        data-field=(chrome.scripts.field.glue) data-field-wasm=(chrome.scripts.field.wasm) {}
                }
                body data-module=(modules) {
                    a.skip href={ "#" (MAIN) } { "Skip to content" }
                    @if places { (draw::place_defs()) }
                    canvas.field aria-hidden="true" data-module="field" {}
                    header.mast {
                        a.wordmark href="/" { (glyph("glyph", "")) span { "Atrium" } }
                        nav.index aria-label="Sections" {
                            ol {
                                @for (i, entry) in chrome.nav.iter().enumerate() {
                                    li {
                                        a href=(entry.href) data-key=(i + 1) data-title=(entry.title)
                                            aria-current=[(current == Some(entry.href)).then_some("page")]
                                        {
                                            span.br aria-hidden="true" { "[" }
                                            span.k { (i + 1) }
                                            span.br aria-hidden="true" { "]" }
                                            span.lb { (entry.label) }
                                        }
                                    }
                                }
                            }
                        }
                        div.mast-tools {
                            time.clock datetime=(local.iso(local.seconds())) data-module="clock" {
                                (local.hm(local.seconds()))
                            }
                            (controls("mast", *prefs))
                            a.jump.btn href=(self.search()) data-module="palette" data-pages=[self.pages()]
                                aria-keyshortcuts="Control+K Meta+K"
                            {
                                span { "Jump" }
                                kbd.kbd-ctrl { "Ctrl K" }
                            }
                        }
                    }
                    (PreEscaped(body))
                    footer.foot {
                        p.foot-note {
                            @match page.foot.or_else(|| (chrome.footnote.0)(local)) {
                                Some(line) => (line),
                                None => "The wiki did not answer.",
                            }
                        }
                        (controls("foot", *prefs))
                        p.foot-keys {
                            kbd { "1" } "–" kbd { (chrome.nav.len()) } " sections, "
                            kbd { "j" } " " kbd { "k" } " rows, "
                            kbd { "/" } " search, "
                            kbd { "Ctrl K" } " jump, "
                            kbd { "T" } " theme, "
                            kbd { "F" } " field"
                        }
                    }
                }
            }
        }
    }
}

/// The greeting the home page's sentence opens with.
#[must_use]
pub const fn greeting(hour: u8) -> &'static str {
    match hour {
        0..5 => "Still up.",
        5..12 => "Good morning.",
        12..18 => "Good afternoon.",
        _ => "Good evening.",
    }
}

/// The home page from every feature's part: the date, the opening sentence, the command
/// line and the drawings above, and the tiles below. When a feature is away the page takes
/// the away look, which the wiki's pages also use when Mneme does not answer.
#[must_use]
pub fn home(shell: &Shell, offers: &[(&'static str, HomeOffer)]) -> Page {
    let away = offers.iter().any(|(_, offer)| offer.away);
    let local = shell.local();
    let names: Vec<String> = offers.iter().map(|(name, _)| name.to_lowercase()).collect();
    let names = names.join(" and ");
    let tiles_label = if away {
        format!("While the {names} is away")
    } else {
        format!("The {names}")
    };
    let body = html! {
        main #(MAIN) .home.away[away] {
            section.hero aria-label="Today" {
                div.lede {
                    h1.label { time datetime=(local.now().date()) { (local.date_long(local.seconds())) } }
                    p.sentence {
                        span data-module="greet" { (greeting(local.hour())) }
                        @for (_, offer) in offers {
                            @for clause in &offer.said { " " (clause) }
                        }
                    }
                    (cmd_line("cmd", shell.search(), shell.pages()))
                }
                @for (_, offer) in offers {
                    @if let Some(figure) = &offer.figure { (figure) }
                }
            }
            section.tiles aria-label=(tiles_label) {
                @for (_, offer) in offers {
                    @for tile in &offer.tiles { (tile) }
                }
            }
        }
    };
    let mut page = Page::new(
        "Atrium",
        "Home: what changed, what wants you, and where you left off.",
        body,
    );
    page.since = !away;
    if away {
        page.description = "The wiki is briefly out of reach. The rest of Atrium works.";
        page.current = Current::Href("/");
        page.foot = Some(html! {
            "The " (names) " did not answer. Everything else here is served by Atrium itself."
        });
    }
    page
}

/// The field intensity, the night theme and the letter keys, as the masthead and the footer
/// both offer them. Each button shows the reader's setting.
fn controls(place: &str, prefs: Prefs) -> Markup {
    let swatch = |level: Intensity, title: &str, label: &str| {
        html! {
            button.swatch type="button" data-intensity=(level.name())
                aria-pressed=(prefs.intensity == level) title=(title)
            {
                (glyph(&format!("sw sw-{}", level.name()), ""))
                span.vh { (label) }
            }
        }
    };
    let dark = prefs.theme == Some(atrium_core::Theme::Dark);
    html! {
        div class={ "prefs prefs-" (place) " js-only" } data-module="prefs" {
            div.intensity role="group" aria-label="Field intensity" {
                span.pref-label aria-hidden="true" { "Field" }
                (swatch(Intensity::Calm, "Calm: the field holds still", "Calm"))
                (swatch(Intensity::Normal, "Normal: the field breathes and follows the pointer", "Normal"))
                (swatch(Intensity::Lively, "Lively: a trail, ripples on click, packets on the lines", "Lively"))
            }
            button.theme-btn type="button" aria-pressed=(dark) title="Night theme" {
                span.theme-ico aria-hidden="true" {}
                span.vh { "Night theme" }
            }
            button.keys-btn type="button" aria-pressed=(prefs.letter_keys)
                title="Single-letter keys: on, keys such as j and 1 work without Ctrl"
            {
                span.keys-ico aria-hidden="true" { "j" }
                span.vh { "Single-letter keys" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use atrium_core::{Prefs, Theme};
    use jiff::tz::TimeZone;

    use super::*;

    fn shell(path: &str, prefs: Prefs) -> Shell {
        let entry = |label, href| NavEntry {
            label,
            title: label,
            href,
        };
        Shell::new(
            Arc::new(Chrome {
                nav: vec![
                    entry("Home", "/"),
                    entry("Wiki", "/wiki"),
                    entry("Changes", "/wiki/changes"),
                ],
                search: Some(Search {
                    action: "/wiki/search",
                    pages: "/wiki/index.json",
                }),
                stylesheet: "/assets/atrium.css".to_owned(),
                fonts: Vec::new(),
                scripts: Scripts::default(),
                footnote: Footnote(Arc::new(|_| None)),
            }),
            path,
            Visit {
                local: Local::new(
                    "2026-11-11T08:41:00Z".parse().unwrap(),
                    TimeZone::get("Europe/Stockholm").unwrap(),
                ),
                last: None,
                prefs,
                entrance: atrium_core::Entrance::Full,
            },
        )
    }

    fn document(path: &str, prefs: Prefs) -> String {
        shell(path, prefs)
            .document(Page::new("t", "d", html! {}))
            .into_string()
    }

    #[test]
    fn marks_the_entry_whose_section_holds_the_path() {
        let page = document("/wiki/page/0123456789abcdef", Prefs::default());
        assert!(
            page.contains(r#"<a href="/wiki" data-key="2" data-title="Wiki" aria-current="page">"#)
        );
        let changes = document("/wiki/changes", Prefs::default());
        assert!(changes.contains(
            r#"<a href="/wiki/changes" data-key="3" data-title="Changes" aria-current="page">"#
        ));
        assert!(changes.contains(r#"<a href="/wiki" data-key="2" data-title="Wiki">"#));
        assert_eq!(
            shell("/wikipedia", Prefs::default()).current(Current::Path),
            None
        );
    }

    #[test]
    fn the_page_carries_the_hour_and_the_readers_settings() {
        let page = document(
            "/",
            Prefs {
                theme: Some(Theme::Dark),
                intensity: Intensity::Calm,
                letter_keys: false,
            },
        );
        assert!(page.contains(r#"<html lang="en" data-hour="9" data-intensity="calm" data-theme="dark" data-keys="off" data-entrance="full">"#), "{page}");
        assert!(
            page.contains(
                r#"class="swatch" type="button" data-intensity="calm" aria-pressed="true""#
            )
        );
        assert!(page.contains(r#"class="keys-btn" type="button" aria-pressed="false""#));
        let plain = document("/", Prefs::default());
        assert!(plain.contains(
            r#"<html lang="en" data-hour="9" data-intensity="normal" data-entrance="full">"#
        ));
    }

    #[test]
    fn the_greeting_follows_the_hour() {
        assert_eq!(greeting(3), "Still up.");
        assert_eq!(greeting(9), "Good morning.");
        assert_eq!(greeting(14), "Good afternoon.");
        assert_eq!(greeting(20), "Good evening.");
    }

    #[test]
    fn the_masthead_clock_reads_the_readers_time() {
        let page = document("/", Prefs::default());
        assert!(page.contains(r#"<time class="clock" datetime="2026-11-11T09:41:00+01:00" data-module="clock">09:41</time>"#), "{page}");
    }
}
