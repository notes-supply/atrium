//! The title you follow becomes the next page's heading during the view transition. Only
//! the clicked link carries the name.

use web_sys::Element;

use crate::dom;

pub fn mount(_: &Element) {
    let (Some(document), Some(window)) = (dom::document(), web_sys::window()) else {
        return;
    };
    dom::on(&document, "click", |event| {
        let Some(link) = dom::closest(&event, "a[data-carry]") else {
            return;
        };
        clear();
        let title = dom::document()
            .and_then(|document| document.query_selector(".doc-title").ok().flatten());
        if title.is_none() {
            let _ = link.class_list().add_1("carry");
        }
    });
    dom::on(&window, "pageshow", |_| clear());
}

fn clear() {
    if let Some(root) = dom::root() {
        for link in dom::query(&root, "a.carry") {
            let _ = link.class_list().remove_1("carry");
        }
    }
}
