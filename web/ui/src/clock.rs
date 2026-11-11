//! The masthead time, to the minute, on the visitor's clock.

use wasm_bindgen::JsCast;
use web_sys::js_sys::Date;
use web_sys::{Element, HtmlTimeElement};

use crate::dom;

pub fn mount(element: &Element) {
    if let Some(time) = element.dyn_ref::<HtmlTimeElement>() {
        tick(time.clone());
    }
}

fn tick(time: HtmlTimeElement) {
    let (hours, minutes) = dom::clock();
    time.set_text_content(Some(&hh_mm(hours, minutes)));
    let now = Date::new_0();
    time.set_date_time(&String::from(now.to_iso_string()));
    #[expect(
        clippy::cast_possible_truncation,
        reason = "under a minute of milliseconds"
    )]
    let wait = (60_000.0 - now.get_time() % 60_000.0) as i32 + 20;
    dom::later(wait, move || tick(time));
}

/// `9, 5` as `09:05`.
#[must_use]
pub fn hh_mm(hours: u32, minutes: u32) -> String {
    let mut text = String::with_capacity(5);
    for (index, part) in [hours, minutes].into_iter().enumerate() {
        if index > 0 {
            text.push(':');
        }
        text.push(char::from_digit(part / 10 % 10, 10).unwrap_or('0'));
        text.push(char::from_digit(part % 10, 10).unwrap_or('0'));
    }
    text
}

#[cfg(test)]
mod tests {
    #[test]
    fn pads_to_two_digits() {
        assert_eq!(super::hh_mm(9, 5), "09:05");
        assert_eq!(super::hh_mm(23, 59), "23:59");
    }
}
