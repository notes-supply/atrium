//! The keyboard. 1 to 5 jump between sections, / searches, j and k move through the rows of
//! a list and Enter opens one, T switches the theme, F steps the field. Hints show while
//! the keyboard is in use. With single-letter keys switched off, only chords work.

use wasm_bindgen::prelude::*;
use web_sys::{
    Element, FocusOptions, HtmlElement, HtmlInputElement, KeyboardEvent, ScrollBehavior,
    ScrollIntoViewOptions, ScrollLogicalPosition,
};

use crate::{dom, palette, prefs};

pub fn mount(_: &Element) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let root = dom::root().ok_or("no root")?;
    let hints = root.clone();
    dom::capture(&window, "keydown", move |event| {
        let key = event.unchecked_ref::<KeyboardEvent>().key();
        if key == "Tab" || key.starts_with("Arrow") || key == "j" || key == "k" {
            let _ = hints.class_list().add_1("kbd");
        }
    });
    dom::capture(&window, "pointerdown", move |_| {
        let _ = root.class_list().remove_1("kbd");
    });
    let platform = window.navigator().platform().unwrap_or_default();
    if ["Mac", "iPhone", "iPad"]
        .iter()
        .any(|name| platform.contains(name))
    {
        for hint in dom::query(&dom::root().ok_or("no root")?, ".kbd-ctrl") {
            hint.set_text_content(Some("⌘K"));
        }
    }
    dom::on(&window, "keydown", |event| {
        let _ = key(event.unchecked_ref());
    });
    Ok(())
}

fn key(event: &KeyboardEvent) -> Option<()> {
    let document = dom::document()?;
    let root = document.document_element()?;
    if event.default_prevented()
        || event.meta_key()
        || event.ctrl_key()
        || event.alt_key()
        || prefs::keys_off()
        || document
            .active_element()
            .is_some_and(|element| typing(&element))
        || document.query_selector("dialog[open]").ok()?.is_some()
    {
        return None;
    }
    let key = event.key();
    if let Some(link) = dom::query(&root, ".index a[data-key]")
        .into_iter()
        .find(|link| link.get_attribute("data-key").as_deref() == Some(key.as_str()))
    {
        event.prevent_default();
        dom::go(&link.get_attribute("href")?);
        return Some(());
    }
    match key.as_str() {
        "/" => {
            event.prevent_default();
            match document.query_selector("#q, .cmd input").ok()? {
                Some(field) if field.get_bounding_client_rect().width() > 0.0 => {
                    let field: HtmlInputElement = field.dyn_into().ok()?;
                    field.focus().ok()?;
                    field.select();
                }
                _ => palette::toggle(),
            }
        }
        "t" | "T" => prefs::toggle_theme(),
        "f" | "F" => prefs::cycle_intensity(),
        "j" | "k" => step(event, &root, key == "j"),
        _ => {}
    }
    Some(())
}

/// Moves the focus to the next or previous row of the list in view.
fn step(event: &KeyboardEvent, root: &Element, down: bool) {
    let rows: Vec<Element> = dom::query(root, "main [data-row]")
        .into_iter()
        .filter(|row| row.get_client_rects().length() > 0)
        .collect();
    let Some(last) = rows.len().checked_sub(1) else {
        return;
    };
    event.prevent_default();
    let active = dom::document().and_then(|document| document.active_element());
    let at = rows.iter().position(|row| Some(row) == active.as_ref());
    let next = match (at, down) {
        (None, true) => 0,
        (None, false) => last,
        (Some(at), true) => (at + 1).min(last),
        (Some(at), false) => at.saturating_sub(1),
    };
    let Some(row) = rows[next].dyn_ref::<HtmlElement>() else {
        return;
    };
    let focus = FocusOptions::new();
    focus.set_prevent_scroll(true);
    let _ = row.focus_with_options(&focus);
    let scroll = ScrollIntoViewOptions::new();
    scroll.set_block(ScrollLogicalPosition::Nearest);
    scroll.set_behavior(if dom::media("(prefers-reduced-motion: reduce)") {
        ScrollBehavior::Auto
    } else {
        ScrollBehavior::Smooth
    });
    row.scroll_into_view_with_scroll_into_view_options(&scroll);
}

/// Whether keys go to a field the reader is typing in.
pub fn typing(element: &Element) -> bool {
    element
        .dyn_ref::<HtmlElement>()
        .is_some_and(HtmlElement::is_content_editable)
        || matches!(element.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
}
