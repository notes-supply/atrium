//! Live search. As you type, the results and their count are replaced in place with the
//! server's own, fetched for the form's query, and the address keeps the query. Without
//! scripts the form submits as usual.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{
    DomParser, Element, FormData, HtmlElement, HtmlFormElement, SupportedType, UrlSearchParams,
};

use crate::dom;

/// The filters, which the summary counts.
const FILTERS: [&str; 6] = ["changed", "created", "vmin", "vmax", "wmin", "wmax"];

/// Keys typed this close together make one request.
const SETTLE_MS: i32 = 60;

pub fn mount(element: &Element) -> Result<(), JsValue> {
    let form: HtmlFormElement = element.clone().dyn_into()?;
    let latest = Rc::new(Cell::new(0_u32));
    let pending = Rc::new(Cell::new(None::<i32>));
    let (typed, waiting, timer) = (form.clone(), Rc::clone(&latest), Rc::clone(&pending));
    dom::on(&form, "input", move |_| {
        cancel(&timer);
        let (form, latest) = (typed.clone(), Rc::clone(&waiting));
        let callback = Closure::once_into_js(move || run(&form, &latest, false));
        timer.set(web_sys::window().and_then(|window| {
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.unchecked_ref(),
                    SETTLE_MS,
                )
                .ok()
        }));
    });
    let submitted = form.clone();
    dom::on(&form, "submit", move |event| {
        event.prevent_default();
        cancel(&pending);
        run(&submitted, &latest, true);
    });
    Ok(())
}

/// Drops a request that typing has scheduled but not sent.
fn cancel(pending: &Cell<Option<i32>>) {
    if let (Some(timer), Some(window)) = (pending.take(), web_sys::window()) {
        window.clear_timeout_with_handle(timer);
    }
}

/// Asks the server for the form's results and swaps them in, unless a newer request went
/// out meanwhile. After a submit, the first result takes the focus.
fn run(form: &HtmlFormElement, latest: &Rc<Cell<u32>>, focus: bool) {
    let Some(query) = query(form) else {
        return;
    };
    let mut page = form.action();
    if let Some(at) = page.find('?') {
        page.truncate(at);
    }
    let address = if query.is_empty() {
        page
    } else {
        [page.as_str(), "?", &query].concat()
    };
    let request = latest.get() + 1;
    latest.set(request);
    let filters = FILTERS
        .iter()
        .filter(|name| {
            query
                .split('&')
                .any(|pair| pair.starts_with(&[name, "="].concat()))
        })
        .count();
    if let Some(summary) = form.query_selector(".filters summary b").ok().flatten() {
        let text = if filters == 0 {
            "none set".to_owned()
        } else {
            [filters.to_string().as_str(), " set"].concat()
        };
        summary.set_text_content(Some(&text));
    }
    let latest = Rc::clone(latest);
    let shown = address.clone();
    dom::fetch(&address, web_sys::Response::text, move |html| {
        if latest.get() != request {
            return;
        }
        let _ = swap(&html.as_string().unwrap_or_default(), &shown, focus);
    });
}

/// The form's fields as a query, leaving out the empty ones.
fn query(form: &HtmlFormElement) -> Option<String> {
    let data = FormData::new_with_form(form).ok()?;
    let params = UrlSearchParams::new_with_str_sequence_sequence(&data).ok()?;
    let filled = UrlSearchParams::new().ok()?;
    for entry in params.entries() {
        let pair = web_sys::js_sys::Array::from(&entry.ok()?);
        let (Some(name), Some(value)) = (pair.get(0).as_string(), pair.get(1).as_string()) else {
            continue;
        };
        if !value.is_empty() {
            filled.append(&name, &value);
        }
    }
    Some(filled.to_string().into())
}

fn swap(html: &str, address: &str, focus: bool) -> Result<(), JsValue> {
    let document = dom::document().ok_or("no document")?;
    let fresh = DomParser::new()?.parse_from_string(html, SupportedType::TextHtml)?;
    for selector in ["[data-swap-target]", "[data-count]"] {
        if let (Some(old), Some(new)) = (
            document.query_selector(selector)?,
            fresh.query_selector(selector)?,
        ) {
            old.set_inner_html(&new.inner_html());
        }
    }
    let window = web_sys::window().ok_or("no window")?;
    let url = web_sys::Url::new(address)?;
    window.history()?.replace_state_with_url(
        &JsValue::NULL,
        "",
        Some(&[url.pathname(), url.search()].concat()),
    )?;
    if focus && let Some(first) = document.query_selector("[data-swap-target] a")? {
        first.unchecked_into::<HtmlElement>().focus()?;
    }
    Ok(())
}
