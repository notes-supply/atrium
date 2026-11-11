//! The field: one fixed WebGL2 canvas behind the page.

use wasm_bindgen::prelude::*;

/// Starts the field on the page's `data-module="field"` canvas.
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
    let _canvas = document.query_selector("canvas[data-module~=field]")?;
    Ok(())
}
