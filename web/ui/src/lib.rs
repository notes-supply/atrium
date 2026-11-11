//! Every small behaviour of Atrium's pages, as one Wasm bundle. Each module mounts on the
//! elements whose `data-module` attribute names it; one element may name several.

use wasm_bindgen::prelude::*;
use web_sys::Element;

mod carry;
mod clock;
mod diff;
mod dom;
mod greet;
mod keys;
mod light;
mod palette;
mod prefs;
mod toc;

type Mount = fn(&Element) -> Result<(), JsValue>;

/// Each module by the name pages give it in `data-module`.
const MODULES: &[(&str, Mount)] = &[
    ("carry", |element| {
        carry::mount(element);
        Ok(())
    }),
    ("clock", |element| {
        clock::mount(element);
        Ok(())
    }),
    ("diff", diff::mount),
    ("greet", |element| {
        greet::mount(element);
        Ok(())
    }),
    ("keys", keys::mount),
    ("light", |element| {
        light::mount(element);
        Ok(())
    }),
    ("palette", palette::mount),
    ("prefs", |element| {
        prefs::mount(element);
        Ok(())
    }),
    ("toc", toc::mount),
];

/// Mounts every module the page names.
///
/// # Errors
///
/// Fails when the page has no document.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;
    let elements = document.query_selector_all("[data-module]")?;
    for index in 0..elements.length() {
        let Some(element) = elements
            .item(index)
            .and_then(|node| node.dyn_into::<Element>().ok())
        else {
            continue;
        };
        let names = element.get_attribute("data-module").unwrap_or_default();
        for name in names.split_whitespace() {
            if let Some((_, mount)) = MODULES.iter().find(|(known, _)| *known == name)
                && let Err(error) = mount(&element)
            {
                web_sys::console::error_2(&JsValue::from_str(name), &error);
            }
        }
    }
    Ok(())
}
