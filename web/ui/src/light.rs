//! The light of the hour. `tokens.css` gives each hour its `--sun` and `--warm` under
//! `data-hour` on `<html>`, which the server writes for its time zone. This module moves the
//! page to the visitor's hour, as it opens and when the hour turns, and eases the light from
//! where it was to the new hour's over `--hour-ease`, forty minutes. It eases in steps: every
//! thirty seconds it sets both numbers on `<html>`, which moves no colour channel by more than
//! one level of 255, and when the ease ends it takes them off, so the hour's own rule holds.
//! Between steps the page does nothing; a transition of the two inherited numbers restyled
//! and repainted the whole page every frame for the forty minutes.
//!
//! Under reduced motion the light takes the new hour at once. A page asked for one hour with
//! `?hour` keeps it.

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::js_sys::{Date, Number, parse_float};
use web_sys::{Element, HtmlElement};

use crate::dom;

/// How often the light steps and the hour is looked at, in milliseconds.
const STEP: i32 = 30_000;
/// The field reads its colours again every this many steps, every two minutes, as it did
/// while a transition eased the light.
const TELL: u32 = 4;
/// The numbers the light is made of, as `tokens.css` names them.
const NUMBERS: [&str; 2] = ["--sun", "--warm"];

/// The light moving from one hour's numbers to the next's, from `begun`, over `length`
/// milliseconds.
struct Ease {
    from: [f64; 2],
    to: [f64; 2],
    begun: f64,
    length: f64,
}

struct Light {
    root: HtmlElement,
    /// A hidden element that takes an hour's rule, so the stylesheet stays the one place
    /// where each hour's numbers are written. Made at the first turn of the hour.
    probe: Option<HtmlElement>,
    ease: Option<Ease>,
    steps: u32,
}

thread_local! {
    static LIGHT: RefCell<Option<Light>> = const { RefCell::new(None) };
}

pub fn mount(_: &Element) {
    if dom::param("hour").is_some() {
        return;
    }
    let Some(root) = dom::root().and_then(|root| root.dyn_into::<HtmlElement>().ok()) else {
        return;
    };
    let started = LIGHT.with_borrow_mut(|light| {
        if light.is_some() {
            return false;
        }
        *light = Some(Light {
            root,
            probe: None,
            ease: None,
            steps: 0,
        });
        true
    });
    if !started {
        return;
    }
    // A hidden tab's timers may wait a minute or more, so a tab shown again catches up at once.
    if let Some(document) = dom::document() {
        dom::on(&document, "visibilitychange", |_| {
            if dom::document().is_some_and(|document| !document.hidden()) {
                step(true);
            }
        });
    }
    follow();
}

fn follow() {
    step(false);
    dom::later(STEP, follow);
}

/// Moves the page to the visitor's hour and the light along its ease, and tells the field
/// when its colours should be read again.
fn step(shown: bool) {
    let tell = LIGHT.with_borrow_mut(|light| light.as_mut().is_some_and(|light| light.step(shown)));
    if tell {
        dom::say("atrium:hour", &JsValue::NULL);
    }
}

impl Light {
    /// Says whether the field should read its colours: on a new hour, when the ease lands,
    /// every `TELL` steps of it, and on a step for a tab `shown` again.
    fn step(&mut self, shown: bool) -> bool {
        let hour = dom::clock().0.to_string();
        let turned = self.root.get_attribute("data-hour").as_deref() != Some(hour.as_str());
        if turned {
            // Held where it is first, so the new hour's rule changes nothing until a step.
            let from = numbers(&self.root);
            hold(&self.root, from);
            let _ = self.root.set_attribute("data-hour", &hour);
            let (to, length) = self.hour(&hour);
            self.ease = Some(Ease {
                from,
                to,
                begun: Date::now(),
                length,
            });
            self.steps = 0;
        }
        let Some(ease) = &self.ease else {
            return false;
        };
        let done = if dom::media("(prefers-reduced-motion: reduce)") {
            1.0
        } else {
            (Date::now() - ease.begun) / ease.length
        };
        // Also when the length did not read as a number.
        if done.is_nan() || done >= 1.0 {
            let style = self.root.style();
            for name in NUMBERS {
                let _ = style.remove_property(name);
            }
            self.ease = None;
            return true;
        }
        let done = done.max(0.0);
        let at = |i: usize| ease.from[i] + (ease.to[i] - ease.from[i]) * done;
        hold(&self.root, [at(0), at(1)]);
        self.steps += 1;
        turned || shown || self.steps.is_multiple_of(TELL)
    }

    /// The hour's numbers as the stylesheet writes them, and how long the light takes to
    /// reach them, in milliseconds.
    fn hour(&mut self, hour: &str) -> ([f64; 2], f64) {
        if self.probe.is_none() {
            self.probe = dom::document().and_then(|document| {
                let probe: HtmlElement = document.create_element("span").ok()?.dyn_into().ok()?;
                probe.set_hidden(true);
                document.body()?.append_child(&probe).ok()?;
                Some(probe)
            });
        }
        let Some(probe) = &self.probe else {
            return (numbers(&self.root), 0.0);
        };
        let _ = probe.set_attribute("data-hour", hour);
        let length = computed(probe, "--hour-ease");
        let length = parse_float(&length) * if length.ends_with("ms") { 1.0 } else { 1000.0 };
        (numbers(probe), length)
    }
}

/// Sets `--sun` and `--warm` on `root`, over the hour's rule.
fn hold(root: &HtmlElement, values: [f64; 2]) {
    let style = root.style();
    for (name, value) in NUMBERS.into_iter().zip(values) {
        if let Ok(text) = Number::from(value).to_fixed(4) {
            let _ = style.set_property(name, &String::from(text));
        }
    }
}

/// `--sun` and `--warm` as they compute on `element`.
fn numbers(element: &Element) -> [f64; 2] {
    NUMBERS.map(|name| parse_float(&computed(element, name)))
}

fn computed(element: &Element, name: &str) -> String {
    web_sys::window()
        .and_then(|window| window.get_computed_style(element).ok().flatten())
        .and_then(|style| style.get_property_value(name).ok())
        .unwrap_or_default()
}
