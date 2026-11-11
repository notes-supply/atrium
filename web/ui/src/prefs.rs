//! The field's intensity, the night theme and the single-letter keys. Each is a cookie, so
//! the server renders the next page in it with scripts off, and an attribute on `<html>`.

use std::cell::Cell;

use wasm_bindgen::prelude::*;
use web_sys::Element;

use crate::dom;

const LEVELS: [&str; 3] = ["calm", "normal", "lively"];

thread_local! {
    static BOUND: Cell<bool> = const { Cell::new(false) };
}

pub fn mount(element: &Element) {
    dom::on(element, "click", |event| {
        if let Some(swatch) = dom::closest(&event, ".swatch") {
            set_intensity(&swatch.get_attribute("data-intensity").unwrap_or_default());
        }
        if dom::closest(&event, ".theme-btn").is_some() {
            toggle_theme();
        }
        if dom::closest(&event, ".keys-btn").is_some() {
            toggle_keys();
        }
    });
    if BOUND.replace(true) {
        return;
    }
    if let Some(list) = web_sys::window().and_then(|window| {
        window
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten()
    }) {
        dom::on(&list, "change", |_| sync());
    }
    sync();
}

pub fn set_intensity(level: &str) {
    set("intensity", level);
}

/// The next level after the current one, from calm through lively and back.
pub fn cycle_intensity() {
    let current = attribute("intensity");
    let at = LEVELS
        .iter()
        .position(|level| *level == current)
        .map_or(0, |at| at + 1);
    set_intensity(LEVELS[at % LEVELS.len()]);
}

pub fn toggle_theme() {
    set("theme", if dark() { "light" } else { "dark" });
}

fn toggle_keys() {
    set("keys", if keys_off() { "on" } else { "off" });
}

pub fn intensity() -> String {
    attribute("intensity")
}

/// Whether the reader switched single-letter shortcuts off.
pub fn keys_off() -> bool {
    attribute("keys") == "off"
}

fn dark() -> bool {
    match attribute("theme").as_str() {
        "" => dom::media("(prefers-color-scheme: dark)"),
        theme => theme == "dark",
    }
}

fn attribute(name: &str) -> String {
    dom::root()
        .and_then(|root| root.get_attribute(&["data-", name].concat()))
        .unwrap_or_default()
}

fn set(name: &str, value: &str) {
    if let Some(root) = dom::root() {
        let _ = root.set_attribute(&["data-", name].concat(), value);
    }
    dom::remember(&["atrium-", name].concat(), value);
    sync();
}

/// Marks every control with the state it shows, outside the components sheet, and tells
/// the field to read its colours again.
fn sync() {
    let Some(root) = dom::root() else {
        return;
    };
    let intensity = intensity();
    for swatch in dom::query(&root, ".swatch[data-intensity]") {
        let on = swatch.get_attribute("data-intensity").as_deref() == Some(intensity.as_str());
        press(&swatch, on);
    }
    let dark = dark();
    for button in dom::query(&root, ".theme-btn") {
        press(&button, dark);
    }
    let keys = !keys_off();
    for button in dom::query(&root, ".keys-btn") {
        press(&button, keys);
    }
    dom::say("atrium:prefs", &JsValue::NULL);
}

fn press(button: &Element, on: bool) {
    if button.closest(".kit").ok().flatten().is_none() {
        let _ = button.set_attribute("aria-pressed", if on { "true" } else { "false" });
    }
}
