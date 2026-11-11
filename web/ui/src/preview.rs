//! A card for a wiki link, anchored to it with CSS anchor positioning. The server writes the
//! cards' data into a JSON block, `#previews`, keyed by each link's `data-preview`.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::js_sys::{JSON, Reflect};
use web_sys::{Element, HtmlElement, KeyboardEvent};

use crate::dom;

struct Preview {
    card: HtmlElement,
    cards: JsValue,
    current: Option<Element>,
    timer: Option<i32>,
}

pub fn mount(article: &Element) -> Result<(), JsValue> {
    let document = dom::document().ok_or("no document")?;
    let Some(data) = document.get_element_by_id("previews") else {
        return Ok(());
    };
    let card: HtmlElement = document.create_element("div")?.dyn_into()?;
    if !web_sys::css::supports_with_value("anchor-name", "--a")?
        || !Reflect::has(&card, &JsValue::from_str("showPopover"))?
    {
        return Ok(());
    }
    card.set_class_name("preview");
    card.set_attribute("popover", "manual")?;
    card.set_attribute("role", "tooltip")?;
    card.set_id("preview");
    document.body().ok_or("no body")?.append_child(&card)?;
    let preview = Rc::new(RefCell::new(Preview {
        card,
        cards: JSON::parse(&data.text_content().unwrap_or_default())?,
        current: None,
        timer: None,
    }));
    for link in dom::query(article, "[data-preview]") {
        let (state, target) = (Rc::clone(&preview), link.clone());
        dom::on(&link, "pointerenter", move |_| {
            let mut preview = state.borrow_mut();
            preview.cancel();
            let (state, target) = (Rc::clone(&state), target.clone());
            let callback = Closure::once_into_js(move || state.borrow_mut().open(&target));
            preview.timer = web_sys::window().and_then(|window| {
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.unchecked_ref(),
                        320,
                    )
                    .ok()
            });
        });
        let state = Rc::clone(&preview);
        dom::on(&link, "pointerleave", move |_| state.borrow_mut().shut());
        let (state, target) = (Rc::clone(&preview), link.clone());
        dom::on(&link, "focus", move |_| state.borrow_mut().open(&target));
        let state = Rc::clone(&preview);
        dom::on(&link, "blur", move |_| state.borrow_mut().shut());
    }
    let window = web_sys::window().ok_or("no window")?;
    dom::on(&window, "keydown", move |event| {
        if event.unchecked_ref::<KeyboardEvent>().key() == "Escape" {
            preview.borrow_mut().shut();
        }
    });
    Ok(())
}

impl Preview {
    fn open(&mut self, link: &Element) {
        let _ = self.show(link);
    }

    fn show(&mut self, link: &Element) -> Result<(), JsValue> {
        let key = link.get_attribute("data-preview").unwrap_or_default();
        let data = Reflect::get(&self.cards, &JsValue::from_str(&key))?;
        if !data.is_object() {
            return Ok(());
        }
        let text = |field: &str| {
            Reflect::get(&data, &JsValue::from_str(field))
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_default()
        };
        if let Some(current) = self.current.replace(link.clone()) {
            current.class_list().remove_1("anchor")?;
        }
        link.class_list().add_1("anchor")?;
        link.set_attribute("aria-describedby", "preview")?;
        let document = dom::document().ok_or("no document")?;
        let span = |class: &str, field: &str| -> Result<web_sys::Node, JsValue> {
            let span = document.create_element("span")?;
            span.set_class_name(class);
            span.set_text_content(Some(&text(field)));
            Ok(span.into())
        };
        self.card.set_text_content(None);
        self.card.append_child(&span("pv-path", "path")?)?;
        self.card.append_child(&span("pv-title", "title")?)?;
        self.card
            .append_child(&document.create_text_node(&text("text")))?;
        self.card.append_child(&span("pv-meta", "meta")?)?;
        if !self.card.matches(":popover-open")? {
            self.card.show_popover()?;
        }
        Ok(())
    }

    fn cancel(&mut self) {
        if let (Some(timer), Some(window)) = (self.timer.take(), web_sys::window()) {
            window.clear_timeout_with_handle(timer);
        }
    }

    fn shut(&mut self) {
        self.cancel();
        if self.card.matches(":popover-open").unwrap_or(false) {
            let _ = self.card.hide_popover();
        }
        if let Some(current) = &self.current {
            let _ = current.remove_attribute("aria-describedby");
        }
    }
}
