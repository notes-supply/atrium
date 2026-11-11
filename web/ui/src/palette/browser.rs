//! The palette's wiring to the page: the dialog, the inline command line on home, and the
//! list of destinations they share.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::js_sys::{self, Array, JsString, Reflect};
use web_sys::{
    Element, HtmlDialogElement, HtmlElement, HtmlInputElement, KeyboardEvent,
    ScrollIntoViewOptions, ScrollLogicalPosition,
};

use super::{Hit, escape, find, mark};
use crate::{dom, prefs};

const SETTINGS: &str = "Settings";

#[derive(Clone)]
enum Action {
    Go(String),
    Theme,
    Intensity(&'static str),
}

#[derive(Clone)]
struct Item {
    group: &'static str,
    title: String,
    /// A second text the query may match, such as a page's path.
    sub: String,
    /// The hint at the row's end: a key, a time, or whether a setting is on.
    hint: String,
    action: Action,
}

/// One list of destinations, in the dialog or inline.
struct Engine {
    input: HtmlInputElement,
    list: Element,
    inline: bool,
    /// The search page, which a query with no page by that name goes to.
    search: String,
    shown: Vec<Item>,
    selected: usize,
}

#[derive(Default)]
struct Shared {
    /// The wiki's pages, once loaded from the URL in a palette element's `data-pages`.
    pages: Vec<Item>,
    loading: bool,
    engines: Vec<Rc<RefCell<Engine>>>,
    dialog: Option<(HtmlDialogElement, Rc<RefCell<Engine>>)>,
    /// The first palette link's address, the search page.
    search: String,
    pages_url: String,
    bound: bool,
}

thread_local! {
    static SHARED: RefCell<Shared> = RefCell::default();
}

/// A link or button opens the dialog; any other element is the inline command line.
pub fn mount(element: &Element) -> Result<(), JsValue> {
    let first = SHARED.with_borrow_mut(|shared| {
        if shared.pages_url.is_empty()
            && let Some(url) = element.get_attribute("data-pages")
        {
            shared.pages_url = url;
        }
        !std::mem::replace(&mut shared.bound, true)
    });
    if first {
        let document = dom::document().ok_or("no document")?;
        dom::on(&document, "keydown", |event| {
            let key = event.unchecked_ref::<KeyboardEvent>();
            if (key.meta_key() || key.ctrl_key()) && key.key().eq_ignore_ascii_case("k") {
                event.prevent_default();
                toggle();
            }
        });
    }
    if element.matches("a, button")? {
        SHARED.with_borrow_mut(|shared| {
            if shared.search.is_empty() {
                shared.search = element.get_attribute("href").unwrap_or_default();
            }
        });
        dom::on(element, "click", |event| {
            event.prevent_default();
            open();
        });
        return Ok(());
    }
    let input: HtmlInputElement = element
        .query_selector("input")?
        .ok_or("no input")?
        .dyn_into()?;
    let list = element.query_selector(".cmd-list")?.ok_or("no list")?;
    let search = element
        .query_selector("form")?
        .and_then(|form| form.get_attribute("action"))
        .unwrap_or_default();
    let engine = Engine::new(&input, &list, true, search);
    let show = {
        let (list, input) = (list.clone(), input.clone());
        move |on: bool| {
            let _ = list.toggle_attribute_with_force("hidden", !on);
            let _ = input.set_attribute("aria-expanded", if on { "true" } else { "false" });
        }
    };
    let (focused, shown) = (Rc::clone(&engine), show.clone());
    dom::on(&input, "focus", move |_| {
        focused.borrow_mut().render();
        shown(true);
    });
    let shown = show.clone();
    dom::on(&input, "input", move |_| shown(true));
    let scope = element.clone();
    dom::on(element, "focusout", move |event| {
        let next = event
            .unchecked_ref::<web_sys::FocusEvent>()
            .related_target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok());
        if !scope.contains(next.as_ref()) {
            show(false);
        }
    });
    Ok(())
}

/// Opens the dialog, or closes it when it is open.
pub fn toggle() {
    let open_now = SHARED.with_borrow(|shared| {
        shared
            .dialog
            .as_ref()
            .is_some_and(|(dialog, _)| dialog.open())
    });
    if open_now {
        close_dialog();
    } else {
        open();
    }
}

fn close_dialog() {
    if let Some(dialog) =
        SHARED.with_borrow(|shared| shared.dialog.as_ref().map(|(d, _)| d.clone()))
    {
        dialog.close();
    }
}

fn open() {
    let _ = try_open();
}

fn try_open() -> Result<(), JsValue> {
    let existing = SHARED.with_borrow(|shared| shared.dialog.clone());
    let (dialog, engine) = if let Some(pair) = existing {
        pair
    } else {
        let pair = build_dialog()?;
        SHARED.with_borrow_mut(|shared| shared.dialog = Some(pair.clone()));
        pair
    };
    let input = engine.borrow().input.clone();
    input.set_value("");
    engine.borrow_mut().render();
    dialog.show_modal()?;
    input.focus()?;
    let window = web_sys::window().ok_or("no window")?;
    let detail = dialog.clone();
    let tell = Closure::once_into_js(move || dom::say("atrium:palette", &detail));
    window.request_animation_frame(tell.unchecked_ref())?;
    Ok(())
}

fn build_dialog() -> Result<(HtmlDialogElement, Rc<RefCell<Engine>>), JsValue> {
    let document = dom::document().ok_or("no document")?;
    let dialog: HtmlDialogElement = document.create_element("dialog")?.dyn_into()?;
    dialog.set_class_name("palette");
    dialog.set_attribute("aria-label", "Jump to")?;
    dialog.set_inner_html(
        "<div class=\"pal-in\"><input type=\"text\" role=\"combobox\" aria-expanded=\"true\" \
         aria-controls=\"pal-list\" aria-autocomplete=\"list\" \
         placeholder=\"Jump to a page, or type a setting\" aria-label=\"Jump to\"><kbd>Esc</kbd></div>\
         <ul class=\"pal-list\" id=\"pal-list\" role=\"listbox\" aria-label=\"Destinations\"></ul>",
    );
    document.body().ok_or("no body")?.append_child(&dialog)?;
    let input: HtmlInputElement = dialog
        .query_selector("input")?
        .ok_or("no input")?
        .dyn_into()?;
    let list = dialog.query_selector("ul")?.ok_or("no list")?;
    let search = SHARED.with_borrow(|shared| shared.search.clone());
    let engine = Engine::new(&input, &list, false, search);
    let backdrop = dialog.clone();
    dom::on(&dialog, "click", move |event| {
        if event.target().as_ref() == Some(backdrop.as_ref()) {
            backdrop.close();
        }
    });
    dom::on(&dialog, "close", |_| {
        dom::say("atrium:palette", &JsValue::NULL);
    });
    Ok((dialog, engine))
}

impl Engine {
    fn new(
        input: &HtmlInputElement,
        list: &Element,
        inline: bool,
        search: String,
    ) -> Rc<RefCell<Self>> {
        let engine = Rc::new(RefCell::new(Self {
            input: input.clone(),
            list: list.clone(),
            inline,
            search,
            shown: Vec::new(),
            selected: 0,
        }));
        SHARED.with_borrow_mut(|shared| shared.engines.push(Rc::clone(&engine)));
        load_pages();

        let keys = Rc::clone(&engine);
        dom::on(input, "input", {
            let engine = Rc::clone(&engine);
            move |_| engine.borrow_mut().render()
        });
        dom::on(input, "keydown", move |event| {
            let key = event.unchecked_ref::<KeyboardEvent>();
            let name = key.key();
            let count = keys.borrow().shown.len();
            let step = match name.as_str() {
                "ArrowDown" => 1,
                "ArrowUp" => count.saturating_sub(1),
                "n" if key.ctrl_key() => 1,
                "p" if key.ctrl_key() => count.saturating_sub(1),
                "Enter" if count > 0 => {
                    event.prevent_default();
                    let selected = keys.borrow().selected;
                    choose(&keys, selected);
                    return;
                }
                "Escape" if keys.borrow().inline => {
                    event.prevent_default();
                    let engine = &keys;
                    if engine.borrow().input.value().is_empty() {
                        close(engine);
                    } else {
                        engine.borrow().input.set_value("");
                        engine.borrow_mut().render();
                    }
                    return;
                }
                _ => return,
            };
            event.prevent_default();
            if count > 0 {
                let mut engine = keys.borrow_mut();
                engine.selected = (engine.selected + step) % count;
                engine.paint();
            }
        });
        let pointer = Rc::clone(&engine);
        dom::on(list, "pointermove", move |event| {
            if let Some(index) = row(&event)
                && pointer.borrow().selected != index
            {
                let mut engine = pointer.borrow_mut();
                engine.selected = index;
                engine.paint();
            }
        });
        dom::on(list, "pointerdown", |event| event.prevent_default());
        let click = Rc::clone(&engine);
        dom::on(list, "click", move |event| {
            if let Some(index) = row(&event) {
                event.prevent_default();
                choose(&click, index);
            }
        });
        engine
    }

    fn render(&mut self) {
        let query = self.input.value().trim().to_owned();
        let lowered = lower(&query);
        let mut rows: Vec<(Item, Hit)> = items()
            .into_iter()
            .filter(|item| !(self.inline && query.is_empty() && item.group == SETTINGS))
            .filter_map(|item| {
                let hit = find(&lowered, &lower(&item.title)).or_else(|| {
                    (!item.sub.is_empty() && find(&lowered, &lower(&item.sub)).is_some()).then(
                        || Hit {
                            score: 0.1,
                            at: Vec::new(),
                        },
                    )
                })?;
                Some((item, hit))
            })
            .collect();
        if !query.is_empty() {
            rows.sort_by(|a, b| b.1.score.total_cmp(&a.1.score));
        }
        rows.truncate(if query.is_empty() { 14 } else { 8 });
        if !query.is_empty() {
            let encoded = String::from(js_sys::encode_uri_component(&query));
            rows.push((
                Item {
                    group: "Search",
                    title: ["Search the wiki for “", &query, "”"].concat(),
                    sub: String::new(),
                    hint: "↵".to_owned(),
                    action: Action::Go([self.search.as_str(), "?q=", &encoded].concat()),
                },
                Hit {
                    score: 0.0,
                    at: Vec::new(),
                },
            ));
        }
        let id = self.list.id();
        let mut html = String::new();
        let mut last = "";
        for (index, (item, hit)) in rows.iter().enumerate() {
            if item.group != last {
                html.push_str("<li class=\"pal-group\" role=\"presentation\">");
                html.push_str(item.group);
                html.push_str("</li>");
                last = item.group;
            }
            let number = index.to_string();
            html.push_str("<li role=\"option\" id=\"");
            escape(&id, &mut html);
            html.push('-');
            html.push_str(&number);
            html.push_str("\" data-i=\"");
            html.push_str(&number);
            html.push_str(
                "\" aria-selected=\"false\"><div class=\"pal-item\"><span class=\"pi-t\">",
            );
            mark(&item.title, &hit.at, &mut html);
            html.push_str("</span><span class=\"pi-m\">");
            escape(&item.hint, &mut html);
            html.push_str("</span></div></li>");
        }
        if html.is_empty() {
            html.push_str(
                "<li class=\"pal-empty\" role=\"presentation\">Nothing by that name.</li>",
            );
        }
        self.list.set_inner_html(&html);
        self.shown = rows.into_iter().map(|(item, _)| item).collect();
        self.selected = 0;
        self.paint();
    }

    fn paint(&self) {
        for option in dom::query(&self.list, "[role=option]") {
            let on = option.get_attribute("data-i") == Some(self.selected.to_string());
            let _ = option.set_attribute("aria-selected", if on { "true" } else { "false" });
            if on {
                let _ = self
                    .input
                    .set_attribute("aria-activedescendant", &option.id());
                let options = ScrollIntoViewOptions::new();
                options.set_block(ScrollLogicalPosition::Nearest);
                option.scroll_into_view_with_scroll_into_view_options(&options);
            }
        }
    }
}

/// The row an event happened in, by its index among the rows shown.
fn row(event: &web_sys::Event) -> Option<usize> {
    dom::closest(event, "[role=option]")?
        .get_attribute("data-i")?
        .parse()
        .ok()
}

fn choose(engine: &Rc<RefCell<Engine>>, index: usize) {
    let Some(action) = engine
        .borrow()
        .shown
        .get(index)
        .map(|item| item.action.clone())
    else {
        return;
    };
    close(engine);
    match action {
        Action::Go(href) => dom::go(&href),
        Action::Theme => prefs::toggle_theme(),
        Action::Intensity(level) => prefs::set_intensity(level),
    }
}

fn close(engine: &Rc<RefCell<Engine>>) {
    let engine = engine.borrow();
    if engine.inline {
        let _ = engine.list.toggle_attribute_with_force("hidden", true);
        let _ = engine.input.set_attribute("aria-expanded", "false");
        let _ = engine.input.unchecked_ref::<HtmlElement>().blur();
    } else {
        close_dialog();
    }
}

/// Every destination: the page's own sections, the wiki's pages, the settings.
fn items() -> Vec<Item> {
    let mut items = Vec::new();
    if let Some(root) = dom::root() {
        for link in dom::query(&root, ".index a[data-key]") {
            let label = link
                .query_selector(".lb")
                .ok()
                .flatten()
                .and_then(|label| label.text_content())
                .unwrap_or_default();
            let key = link.get_attribute("data-key").unwrap_or_default();
            items.push(Item {
                group: "Sections",
                title: label,
                sub: String::new(),
                hint: ["[", &key, "]"].concat(),
                action: Action::Go(link.get_attribute("href").unwrap_or_default()),
            });
        }
    }
    SHARED.with_borrow(|shared| items.extend(shared.pages.iter().cloned()));
    items.push(Item {
        group: SETTINGS,
        title: "Switch day and night".to_owned(),
        sub: String::new(),
        hint: "T".to_owned(),
        action: Action::Theme,
    });
    let current = prefs::intensity();
    for level in ["calm", "normal", "lively"] {
        items.push(Item {
            group: SETTINGS,
            title: ["Field: ", level].concat(),
            sub: String::new(),
            hint: if current == level { "on" } else { "" }.to_owned(),
            action: Action::Intensity(level),
        });
    }
    items
}

/// Lowercase as the browser does it, which knows every script.
fn lower(text: &str) -> String {
    JsString::from(text).to_lower_case().into()
}

/// Fetches the pages once, then renders any list whose field has focus.
fn load_pages() {
    let Some(url) = SHARED.with_borrow_mut(|shared| {
        if shared.loading || shared.pages_url.is_empty() {
            return None;
        }
        shared.loading = true;
        Some(shared.pages_url.clone())
    }) else {
        return;
    };
    dom::fetch(&url, web_sys::Response::json, |list| {
        let pages: Vec<Item> = Array::from(&list)
            .iter()
            .map(|page| {
                let text = |key: &str| {
                    Reflect::get(&page, &JsValue::from_str(key))
                        .ok()
                        .and_then(|value| value.as_string())
                        .unwrap_or_default()
                };
                Item {
                    group: "Pages",
                    title: text("t"),
                    sub: text("path"),
                    hint: text("m"),
                    action: Action::Go(text("href")),
                }
            })
            .collect();
        let engines = SHARED.with_borrow_mut(|shared| {
            shared.pages = pages;
            shared.engines.clone()
        });
        let active = dom::document().and_then(|document| document.active_element());
        for engine in engines {
            let focused = active.as_ref() == Some(engine.borrow().input.as_ref());
            if focused {
                engine.borrow_mut().render();
            }
        }
    });
}
