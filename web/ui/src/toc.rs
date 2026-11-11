//! The contents follow the reading: the heading in view is the current link. Closed on a
//! phone.

use std::collections::HashSet;

use wasm_bindgen::prelude::*;
use web_sys::js_sys::Array;
use web_sys::{
    Element, HtmlDetailsElement, IntersectionObserver, IntersectionObserverEntry,
    IntersectionObserverInit,
};

use crate::dom;

pub fn mount(details: &Element) -> Result<(), JsValue> {
    if dom::media("(max-width: 899px)")
        && let Some(details) = details.dyn_ref::<HtmlDetailsElement>()
    {
        details.set_open(false);
    }
    let document = dom::document().ok_or("no document")?;
    let mut links = Vec::new();
    let mut heads = Vec::new();
    for link in dom::query(details, "a[href^=\"#\"]") {
        let id = link.get_attribute("href").unwrap_or_default()[1..].to_owned();
        if let Some(head) = document.get_element_by_id(&id) {
            heads.push(head);
            links.push((id, link));
        }
    }
    if heads.is_empty() {
        return Ok(());
    }
    let targets = heads.clone();
    let mut seen = HashSet::new();
    let callback = Closure::<dyn FnMut(Array)>::new(move |entries: Array| {
        for entry in entries.iter() {
            let entry: IntersectionObserverEntry = entry.unchecked_into();
            let id = entry.target().id();
            if entry.is_intersecting() {
                seen.insert(id);
            } else {
                seen.remove(&id);
            }
        }
        let current = heads
            .iter()
            .find(|head| seen.contains(&head.id()))
            .or_else(|| {
                heads
                    .iter()
                    .rfind(|head| head.get_bounding_client_rect().top() < 120.0)
            })
            .unwrap_or(&heads[0])
            .id();
        for (id, link) in &links {
            let _ = link.set_attribute(
                "aria-current",
                if *id == current { "true" } else { "false" },
            );
        }
    });
    let options = IntersectionObserverInit::new();
    options.set_root_margin("-80px 0px -55% 0px");
    let observer =
        IntersectionObserver::new_with_options(callback.as_ref().unchecked_ref(), &options)?;
    callback.forget();
    for head in &targets {
        observer.observe(head);
    }
    Ok(())
}
