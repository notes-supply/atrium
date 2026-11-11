//! The palette's wiring to the page: a `<dialog>` with a filter field and a copy of the
//! page's command links, opened by Ctrl+K or Cmd+K, and the key sequences outside it.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlDialogElement, HtmlElement, HtmlInputElement, KeyboardEvent};

use super::{Step, matches, step};

/// Keys typed further apart than this many milliseconds start a new sequence.
const PAUSE_MS: f64 = 1500.0;

struct Command {
    label: String,
    keys: Option<String>,
    href: String,
    /// The command's item in the dialog's list.
    item: Element,
    link: HtmlElement,
}

struct Palette {
    document: Document,
    dialog: HtmlDialogElement,
    input: HtmlInputElement,
    commands: Vec<Command>,
    typed: String,
    last_key_at: f64,
}

/// Builds the palette from the command links in `source`.
pub fn mount(source: &Element) -> Result<(), JsValue> {
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;
    let dialog: HtmlDialogElement = document.create_element("dialog")?.dyn_into()?;
    dialog.set_class_name("palette");
    dialog.set_attribute("aria-label", "Commands")?;
    let input: HtmlInputElement = document.create_element("input")?.dyn_into()?;
    input.set_type("search");
    input.set_attribute("aria-label", "Filter the commands")?;
    input.set_attribute("autocomplete", "off")?;
    let list = document.create_element("ul")?;
    dialog.append_child(&input)?;
    dialog.append_child(&list)?;

    let links = source.query_selector_all("a[href]")?;
    let mut commands = Vec::new();
    for index in 0..links.length() {
        let Some(original) = links.item(index) else {
            continue;
        };
        let link: HtmlElement = original.clone_node_with_deep(true)?.dyn_into()?;
        let item = document.create_element("li")?;
        item.append_child(&link)?;
        list.append_child(&item)?;
        commands.push(Command {
            label: link.text_content().unwrap_or_default(),
            keys: link.get_attribute("data-keys"),
            href: link.get_attribute("href").unwrap_or_default(),
            item,
            link,
        });
    }
    document.body().ok_or("no body")?.append_child(&dialog)?;

    let palette = Rc::new(RefCell::new(Palette {
        document: document.clone(),
        dialog,
        input: input.clone(),
        commands,
        typed: String::new(),
        last_key_at: f64::NEG_INFINITY,
    }));
    let keys = Rc::clone(&palette);
    let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
        let _ = keys.borrow_mut().key(&event);
    });
    document.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref())?;
    // The listeners live as long as the page.
    on_key.forget();
    let on_input = Closure::<dyn FnMut()>::new(move || palette.borrow().filter());
    input.add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref())?;
    on_input.forget();
    Ok(())
}

impl Palette {
    fn key(&mut self, event: &KeyboardEvent) -> Result<(), JsValue> {
        let key = event.key();
        let chord = event.ctrl_key() || event.meta_key();
        if chord && key.eq_ignore_ascii_case("k") {
            event.prevent_default();
            return self.toggle();
        }
        if self.dialog.open() {
            return self.move_within(event, &key);
        }
        if chord || event.alt_key() || typing(event) {
            return Ok(());
        }
        let now = event.time_stamp();
        let typed = if now - self.last_key_at > PAUSE_MS {
            ""
        } else {
            self.typed.as_str()
        };
        self.last_key_at = now;
        let sequences: Vec<Option<&str>> = self
            .commands
            .iter()
            .map(|command| command.keys.as_deref())
            .collect();
        match step(typed, &key, &sequences) {
            Step::Run(index) => {
                self.typed.clear();
                event.prevent_default();
                go(&self.commands[index].href)
            }
            Step::Wait(typed) => {
                self.typed = typed;
                Ok(())
            }
            Step::Nothing => {
                self.typed.clear();
                Ok(())
            }
        }
    }

    fn toggle(&self) -> Result<(), JsValue> {
        if self.dialog.open() {
            self.dialog.close();
            return Ok(());
        }
        self.input.set_value("");
        self.filter();
        self.dialog.show_modal()?;
        self.input.focus()
    }

    /// Enter in the field follows the first command shown; the arrows move between the field
    /// and the commands shown.
    fn move_within(&self, event: &KeyboardEvent, key: &str) -> Result<(), JsValue> {
        let shown: Vec<&Command> = self
            .commands
            .iter()
            .filter(|command| !command.item.has_attribute("hidden"))
            .collect();
        let active = self.document.active_element();
        let at = shown
            .iter()
            .position(|command| active.as_ref() == Some(command.link.as_ref()));
        let target = match (key, at) {
            ("Enter", None) => return shown.first().map_or(Ok(()), |first| go(&first.href)),
            ("ArrowDown", None) => shown.first().map(|command| &command.link),
            ("ArrowDown", Some(at)) => shown.get(at + 1).map(|command| &command.link),
            ("ArrowUp", Some(0)) => Some(self.input.unchecked_ref::<HtmlElement>()),
            ("ArrowUp", Some(at)) => shown.get(at - 1).map(|command| &command.link),
            _ => return Ok(()),
        };
        event.prevent_default();
        target.map_or(Ok(()), HtmlElement::focus)
    }

    fn filter(&self) {
        let query = self.input.value();
        for command in &self.commands {
            let _ = command
                .item
                .toggle_attribute_with_force("hidden", !matches(&query, &command.label));
        }
    }
}

/// Whether the key goes to a field the person is typing in.
fn typing(event: &KeyboardEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<HtmlElement>().ok())
        .is_some_and(|element| {
            element.is_content_editable()
                || matches!(element.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
        })
}

fn go(href: &str) -> Result<(), JsValue> {
    web_sys::window()
        .ok_or("no window")?
        .location()
        .assign(href)
}
