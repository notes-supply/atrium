//! The few ways every module touches the page.

use wasm_bindgen::prelude::*;
use web_sys::js_sys;
use web_sys::{CustomEvent, CustomEventInit, Document, Element, Event, EventTarget, HtmlDocument};

pub fn document() -> Option<Document> {
    web_sys::window()?.document()
}

/// The `<html>` element, where the preferences and the hour live.
pub fn root() -> Option<Element> {
    document()?.document_element()
}

/// Runs `listener` on every `kind` event at `target`, for as long as the page lives.
pub fn on(target: &EventTarget, kind: &str, listener: impl FnMut(Event) + 'static) {
    listen(target, kind, false, listener);
}

/// As [`on`], but before the event reaches its target.
pub fn capture(target: &EventTarget, kind: &str, listener: impl FnMut(Event) + 'static) {
    listen(target, kind, true, listener);
}

fn listen(target: &EventTarget, kind: &str, capture: bool, listener: impl FnMut(Event) + 'static) {
    let closure = Closure::<dyn FnMut(Event)>::new(listener);
    let _ = target.add_event_listener_with_callback_and_bool(
        kind,
        closure.as_ref().unchecked_ref(),
        capture,
    );
    closure.forget();
}

/// Tells the other bundle, which cannot call this one, that something changed.
pub fn say(kind: &str, detail: &JsValue) {
    let init = CustomEventInit::new();
    init.set_detail(detail);
    if let (Some(document), Ok(event)) = (
        document(),
        CustomEvent::new_with_event_init_dict(kind, &init),
    ) {
        let _ = document.dispatch_event(&event);
    }
}

/// Stores a preference for a year, where the server reads it on the next request.
pub fn remember(name: &str, value: &str) {
    if let Some(document) = document().and_then(|document| document.dyn_into::<HtmlDocument>().ok())
    {
        let _ = document
            .set_cookie(&[name, "=", value, "; path=/; max-age=31536000; samesite=lax"].concat());
    }
}

/// The nearest element to the event's target that matches `selector`.
pub fn closest(event: &Event, selector: &str) -> Option<Element> {
    event
        .target()?
        .dyn_into::<Element>()
        .ok()?
        .closest(selector)
        .ok()?
}

pub fn query(scope: &Element, selector: &str) -> Vec<Element> {
    let Ok(list) = scope.query_selector_all(selector) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|index| list.item(index)?.dyn_into().ok())
        .collect()
}

pub fn media(query: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.match_media(query).ok().flatten())
        .is_some_and(|list| list.matches())
}

pub fn go(href: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(href);
    }
}

#[wasm_bindgen]
extern "C" {
    type Thenable;
    #[wasm_bindgen(method)]
    fn then(this: &Thenable, ok: &JsValue, failed: &JsValue) -> Thenable;
}

/// Fetches `url` and calls `done` with its body parsed as JSON, unless the request fails or
/// answers an error. A failure is quiet: the page keeps what the server wrote.
pub fn fetch(url: &str, done: impl FnOnce(JsValue) + 'static) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let read = Closure::once_into_js(move |response: web_sys::Response| -> JsValue {
        match response.json() {
            Ok(promise) if response.ok() => promise.into(),
            _ => JsValue::UNDEFINED,
        }
    });
    let done = Closure::once_into_js(move |value: JsValue| {
        if !value.is_undefined() {
            done(value);
        }
    });
    let quiet = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    window
        .fetch_with_str(url)
        .unchecked_into::<Thenable>()
        .then(&read, &JsValue::UNDEFINED)
        .then(&done, quiet.as_ref());
    quiet.forget();
}

/// The value of `name` in the address's query, such as `only` in `?only=changed`.
pub fn param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get(name)
}

/// Runs `then` once, after `ms` milliseconds.
pub fn later(ms: i32, then: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let callback = Closure::once_into_js(then);
        let _ = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), ms);
    }
}

/// The visitor's clock: the hour and minute in their own time zone.
pub fn clock() -> (u32, u32) {
    let now = js_sys::Date::new_0();
    (now.get_hours(), now.get_minutes())
}
