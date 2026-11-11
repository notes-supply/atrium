//! The sentence's greeting follows the visitor's clock when it disagrees with the server's
//! time zone.

use web_sys::Element;

use crate::dom;

pub fn mount(element: &Element) {
    element.set_text_content(Some(greeting(dom::clock().0)));
}

#[must_use]
pub fn greeting(hour: u32) -> &'static str {
    match hour {
        0..5 => "Still up.",
        5..12 => "Good morning.",
        12..18 => "Good afternoon.",
        _ => "Good evening.",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn by_the_hour() {
        assert_eq!(super::greeting(4), "Still up.");
        assert_eq!(super::greeting(5), "Good morning.");
        assert_eq!(super::greeting(12), "Good afternoon.");
        assert_eq!(super::greeting(18), "Good evening.");
    }
}
