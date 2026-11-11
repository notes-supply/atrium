//! The diff's "only changed paragraphs" link becomes a toggle in place.

use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::dom;

pub fn mount(section: &Element) -> Result<(), JsValue> {
    let Some(only) = section.query_selector("[data-only-changes]")? else {
        return Ok(());
    };
    only.set_attribute("role", "button")?;
    set(
        section,
        &only,
        dom::param("only").as_deref() == Some("changed"),
    );
    let (scope, toggle) = (section.clone(), only.clone());
    dom::on(&only, "click", move |event| {
        event.prevent_default();
        let on = toggle.get_attribute("aria-pressed").as_deref() != Some("true");
        set(&scope, &toggle, on);
    });
    let link = only.clone();
    dom::on(&only, "keydown", move |event| {
        if event.unchecked_ref::<KeyboardEvent>().key() == " " {
            event.prevent_default();
            link.unchecked_ref::<HtmlElement>().click();
        }
    });
    Ok(())
}

fn set(section: &Element, only: &Element, on: bool) {
    let _ = only.set_attribute("aria-pressed", if on { "true" } else { "false" });
    let _ = section.class_list().toggle_with_force("only-changes", on);
}
