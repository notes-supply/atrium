//! Moves `data-hour` on `<html>` to the visitor's hour, and again when the hour turns while a
//! page stays open. `tokens.css` eases the light over forty minutes. A page asked for one
//! hour with `?hour` keeps it.

use std::cell::Cell;

use wasm_bindgen::JsValue;
use web_sys::Element;

use crate::dom;

thread_local! {
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

pub fn mount(_: &Element) {
    if STARTED.replace(true) || dom::param("hour").is_some() {
        return;
    }
    follow(0);
}

/// Sets the hour each minute, and every second minute asks the field to read its colours
/// again while the light eases.
fn follow(minute: u32) {
    let hour = dom::clock().0.to_string();
    if let Some(root) = dom::root()
        && root.get_attribute("data-hour").as_deref() != Some(hour.as_str())
    {
        let _ = root.set_attribute("data-hour", &hour);
        dom::say("atrium:hour", &JsValue::NULL);
    } else if minute % 2 == 1 {
        dom::say("atrium:hour", &JsValue::NULL);
    }
    dom::later(60_000, move || follow(minute + 1));
}
