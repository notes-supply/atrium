//! The theme, the field's intensity and the single-letter keys. Each is a cookie, so the
//! server renders the next page in it with scripts off, and an attribute on `<html>`. The
//! settings page's buttons post a choice; with scripts on, this takes it in place.

use wasm_bindgen::prelude::*;
use web_sys::Element;

use crate::dom;

const LEVELS: [&str; 3] = ["calm", "normal", "lively"];

/// The buttons that each post one choice, as `name=value`.
const CHOICE: &str = "form.prefs button[name][value]";

/// Every value each setting takes, as the server accepts them.
const SETTINGS: [(&str, &[&str]); 3] = [
    ("theme", &["system", "light", "dark"]),
    ("intensity", &["calm", "normal", "lively"]),
    ("keys", &["on", "off"]),
];

pub fn mount(element: &Element) {
    dom::on(element, "click", |event| {
        let Some(button) = dom::closest(&event, CHOICE) else {
            return;
        };
        let name = button.get_attribute("name").unwrap_or_default();
        let value = button.get_attribute("value").unwrap_or_default();
        if known(&name, &value) {
            event.prevent_default();
            choose(&name, &value);
        }
    });
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

fn known(name: &str, value: &str) -> bool {
    SETTINGS
        .iter()
        .any(|(setting, values)| *setting == name && values.contains(&value))
}

/// Takes a choice: following the system forgets the theme, any other is kept.
fn choose(name: &str, value: &str) {
    if (name, value) == ("theme", "system") {
        if let Some(root) = dom::root() {
            let _ = root.remove_attribute("data-theme");
        }
        dom::forget("atrium-theme");
        sync();
    } else {
        set(name, value);
    }
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

/// The value of `name` the reader has now, as its buttons name it.
fn current(name: &str) -> String {
    let value = attribute(name);
    match (name, value.as_str()) {
        ("theme", "") => "system".to_owned(),
        ("intensity", "") => "normal".to_owned(),
        ("keys", "off") => "off".to_owned(),
        ("keys", _) => "on".to_owned(),
        _ => value,
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

/// Presses the button of each setting's current value, and tells the field to read its
/// colours again.
fn sync() {
    let Some(root) = dom::root() else {
        return;
    };
    for button in dom::query(&root, CHOICE) {
        let name = button.get_attribute("name").unwrap_or_default();
        let on = button.get_attribute("value") == Some(current(&name));
        let _ = button.set_attribute("aria-pressed", if on { "true" } else { "false" });
    }
    dom::say("atrium:prefs", &JsValue::NULL);
}
