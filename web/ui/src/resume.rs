//! Continue reading. On a page, remembers the heading you reached and how far through you
//! were, in this browser's storage. On home, fills the "Continue reading" tile from it; on a
//! page you read before, fills the note that says where you were. With scripts off, or with
//! nothing read yet, both stay hidden.
//!
//! The module is `continue`, a Rust keyword, so the file is `resume`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::js_sys::Date;
use web_sys::{Element, Storage};

use crate::dom;
use crate::palette::escape;

const KEY: &str = "atrium-reading";
/// Pages remembered at most; the oldest goes first.
const KEEP: usize = 20;
/// A page read this far is finished, and no longer offered.
const FINISHED: u32 = 95;

/// Where the reader stopped on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub page: String,
    pub href: String,
    pub title: String,
    /// The last heading above the reading line, as its id and its text.
    pub heading: Option<(String, String)>,
    pub percent: u32,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
}

/// Reads the places stored as one line each, fields split by tabs.
#[must_use]
pub fn parse(stored: &str) -> Vec<Place> {
    stored
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let mut next = || fields.next().map(str::to_owned);
            let (page, href, title, id, text) = (next()?, next()?, next()?, next()?, next()?);
            Some(Place {
                page,
                href,
                title,
                heading: (!id.is_empty()).then_some((id, text)),
                percent: next()?.parse().ok()?,
                at: next()?.parse().ok()?,
            })
        })
        .collect()
}

#[must_use]
pub fn serialize(places: &[Place]) -> String {
    let mut out = String::new();
    for place in places {
        let (id, text) = place
            .heading
            .as_ref()
            .map_or(("", ""), |(id, text)| (id.as_str(), text.as_str()));
        for field in [&place.page, &place.href, &place.title, id, text] {
            out.extend(field.chars().map(|c| if c.is_control() { ' ' } else { c }));
            out.push('\t');
        }
        out.push_str(&place.percent.to_string());
        out.push('\t');
        out.push_str(&place.at.to_string());
        out.push('\n');
    }
    out
}

/// Puts `place` first, replacing the page's older place, and keeps the newest few.
pub fn record(places: &mut Vec<Place>, place: Place) {
    places.retain(|old| old.page != place.page);
    places.insert(0, place);
    places.truncate(KEEP);
}

/// The places worth offering, newest first.
pub fn unfinished(places: &[Place]) -> impl Iterator<Item = &Place> {
    places.iter().filter(|place| place.percent < FINISHED)
}

/// How long ago `ms` milliseconds was, as a person says it.
#[must_use]
pub fn ago(ms: u64) -> String {
    let minutes = ms / 60_000;
    let (count, unit) = match minutes {
        0 => return "just now".to_owned(),
        1..60 => (minutes, "minute"),
        60..1440 => (minutes / 60, "hour"),
        1440..2880 => return "yesterday".to_owned(),
        _ => (minutes / 1440, "day"),
    };
    let plural = if count == 1 { "" } else { "s" };
    [count.to_string().as_str(), " ", unit, plural, " ago"].concat()
}

/// Milliseconds since the Unix epoch.
fn now() -> u64 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a time after 1970 in whole milliseconds"
    )]
    let now = Date::now() as u64;
    now
}

/// Rounds a percentage to the steps the stylesheet draws, every 5.
#[must_use]
pub fn step(percent: u32) -> u32 {
    (percent + 2) / 5 * 5
}

pub fn mount(element: &Element) -> Result<(), JsValue> {
    let storage = web_sys::window()
        .ok_or("no window")?
        .local_storage()?
        .ok_or("no storage")?;
    match element.get_attribute("data-page") {
        Some(page) => follow(element, page, storage),
        None => fill_tile(element, &storage),
    }
}

fn load(storage: &Storage) -> Vec<Place> {
    parse(&storage.get_item(KEY).ok().flatten().unwrap_or_default())
}

/// On a page: says where the reader was before, then remembers where they are.
fn follow(article: &Element, page: String, storage: Storage) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    if let Some(before) = load(&storage).into_iter().find(|place| place.page == page)
        && let Some(note) = article.query_selector(".resume-note[hidden]")?
        && let Some(span) = note.query_selector("span")?
        && let Some((id, text)) = &before.heading
    {
        let mut html = String::from("You were here before, at <a href=\"#");
        escape(id, &mut html);
        html.push_str("\">");
        escape(text, &mut html);
        html.push_str("</a>, ");
        html.push_str(&before.percent.to_string());
        html.push_str("% through.");
        span.set_inner_html(&html);
        note.remove_attribute("hidden")?;
    }
    let title = article
        .query_selector(".doc-title")?
        .and_then(|title| title.text_content())
        .unwrap_or_default();
    let href = window.location().pathname()?;
    let heads = dom::query(article, "h2[id]");
    let save = {
        let (article, window) = (article.clone(), window.clone());
        move || {
            let height = window
                .inner_height()
                .ok()
                .and_then(|height| height.as_f64())
                .unwrap_or_default();
            let line = height * 0.3;
            let rect = article.get_bounding_client_rect();
            // The reading line never reaches the end of a short page, so its end in view
            // counts as read through.
            let through = if rect.bottom() <= height {
                1.0
            } else {
                ((line - rect.top()) / rect.height()).clamp(0.0, 1.0)
            };
            let heading = heads
                .iter()
                .rfind(|head| head.get_bounding_client_rect().top() < line)
                .map(|head| (head.id(), head.text_content().unwrap_or_default()));
            let mut places = load(&storage);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a share between 0 and 1, times 100"
            )]
            let percent = (through * 100.0).round() as u32;
            record(
                &mut places,
                Place {
                    page: page.clone(),
                    href: href.clone(),
                    title: title.clone(),
                    heading,
                    percent,
                    at: now(),
                },
            );
            let _ = storage.set_item(KEY, &serialize(&places));
        }
    };
    let save = Rc::new(save);
    let pending = Rc::new(RefCell::new(None::<i32>));
    let on_scroll = Rc::clone(&save);
    dom::on(&window, "scroll", move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };
        if let Some(timer) = pending.borrow_mut().take() {
            window.clear_timeout_with_handle(timer);
        }
        let save = Rc::clone(&on_scroll);
        let callback = Closure::once_into_js(move || save());
        *pending.borrow_mut() = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 600)
            .ok();
    });
    dom::on(&window, "pagehide", move |_| save());
    Ok(())
}

/// On home: the newest unfinished page, how far through, and a few more.
fn fill_tile(tile: &Element, storage: &Storage) -> Result<(), JsValue> {
    let places = load(storage);
    let mut offered = unfinished(&places);
    let Some(first) = offered.next() else {
        return Ok(());
    };
    let percent = first.percent.to_string();
    let step = step(first.percent).to_string();
    let mut html = String::from("<p class=\"resume-title\"><a href=\"");
    escape(&first.href, &mut html);
    html.push_str("\" data-carry>");
    escape(&first.title, &mut html);
    html.push_str("</a></p><p class=\"resume-where\">You stopped ");
    if let Some((id, text)) = &first.heading {
        html.push_str("at <a href=\"");
        escape(&first.href, &mut html);
        html.push('#');
        escape(id, &mut html);
        html.push_str("\">");
        escape(text, &mut html);
        html.push_str("</a>");
    } else {
        html.push_str("at the start");
    }
    html.push_str(", ");
    html.push_str(&ago(now().saturating_sub(first.at)));
    html.push_str(".</p><div class=\"progress-bar\" role=\"img\" aria-label=\"");
    html.push_str(&percent);
    html.push_str(" percent read\"><span class=\"pb-fill pct-");
    html.push_str(&step);
    html.push_str("\"></span><span class=\"pb-n pct-");
    html.push_str(&step);
    html.push_str("\" aria-hidden=\"true\">");
    html.push_str(&percent);
    html.push_str("%</span></div>");
    let more: Vec<&Place> = offered.take(3).collect();
    if !more.is_empty() {
        html.push_str("<ul class=\"resume-more\">");
        for place in more {
            html.push_str("<li><a href=\"");
            escape(&place.href, &mut html);
            if let Some((id, _)) = &place.heading {
                html.push('#');
                escape(id, &mut html);
            }
            html.push_str("\">");
            escape(&place.title, &mut html);
            html.push_str("</a><span class=\"pct\">");
            html.push_str(&place.percent.to_string());
            html.push_str("%</span></li>");
        }
        html.push_str("</ul>");
    }
    tile.insert_adjacent_html("beforeend", &html)?;
    tile.remove_attribute("hidden")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(page: &str, percent: u32) -> Place {
        Place {
            page: page.to_owned(),
            href: ["/wiki/page/", page].concat(),
            title: "Restic\tbackups".to_owned(),
            heading: Some(("restore".to_owned(), "Restore a file".to_owned())),
            percent,
            at: 1_762_850_000_000,
        }
    }

    #[test]
    fn places_survive_storage_without_their_control_characters() {
        let stored = serialize(&[
            place("a", 40),
            Place {
                heading: None,
                ..place("b", 0)
            },
        ]);
        let read = parse(&stored);
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].title, "Restic backups");
        assert_eq!(read[0].heading, place("a", 40).heading);
        assert_eq!(read[0].at, 1_762_850_000_000);
        assert_eq!(read[1].heading, None);
    }

    #[test]
    fn a_damaged_line_is_skipped() {
        assert_eq!(parse("a\tb\n\n"), []);
    }

    #[test]
    fn the_newest_place_comes_first_once_per_page() {
        let mut places = vec![place("a", 10), place("b", 20)];
        record(&mut places, place("b", 30));
        assert_eq!(
            places
                .iter()
                .map(|p| (p.page.as_str(), p.percent))
                .collect::<Vec<_>>(),
            [("b", 30), ("a", 10)]
        );
        for page in 0..30 {
            record(&mut places, place(&page.to_string(), 1));
        }
        assert_eq!(places.len(), KEEP);
    }

    #[test]
    fn a_finished_page_is_not_offered() {
        let places = [place("a", 95), place("b", 94)];
        assert_eq!(
            unfinished(&places)
                .map(|p| p.page.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
    }

    #[test]
    fn says_how_long_ago() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(60_000), "1 minute ago");
        assert_eq!(ago(59 * 60_000), "59 minutes ago");
        assert_eq!(ago(3 * 3_600_000), "3 hours ago");
        assert_eq!(ago(30 * 3_600_000), "yesterday");
        assert_eq!(ago(50 * 3_600_000), "2 days ago");
    }

    #[test]
    fn rounds_to_the_drawn_steps() {
        assert_eq!(step(0), 0);
        assert_eq!(step(62), 60);
        assert_eq!(step(63), 65);
        assert_eq!(step(100), 100);
    }
}
