//! The field's colours, read from the stylesheet's tokens through a 2D canvas, so any CSS
//! colour syntax (oklch, light-dark) comes back as sRGB bytes.

use wasm_bindgen::prelude::*;
use web_sys::{
    CanvasRenderingContext2d, ContextAttributes2d, Document, HtmlCanvasElement, HtmlElement,
};

/// Ground, dot, warm and hot, in the order the lattice pass takes them.
const TOKENS: [&str; 4] = ["--ground", "--field-dot", "--field-warm", "--bad"];

pub struct Sampler {
    probe: HtmlElement,
    context: CanvasRenderingContext2d,
}

impl Sampler {
    pub fn new(document: &Document) -> Result<Self, JsValue> {
        let probe: HtmlElement = document.create_element("span")?.dyn_into()?;
        probe.set_attribute("aria-hidden", "true")?;
        probe.set_class_name("vh probe");
        document.body().ok_or("no body")?.append_child(&probe)?;
        let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
        canvas.set_width(1);
        canvas.set_height(1);
        let settings = ContextAttributes2d::new();
        settings.set_will_read_frequently(true);
        let context = canvas
            .get_context_with_context_options("2d", &settings)?
            .ok_or("no 2d context")?
            .dyn_into()?;
        Ok(Self { probe, context })
    }

    /// Each token's colour as it computes now, between 0 and 1.
    pub fn read(&self) -> [[f32; 3]; 4] {
        let window = web_sys::window();
        TOKENS.map(|token| {
            let _ = self
                .probe
                .style()
                .set_property("color", &["var(", token, ")"].concat());
            let color = window
                .as_ref()
                .and_then(|window| window.get_computed_style(&self.probe).ok().flatten())
                .and_then(|style| style.get_property_value("color").ok())
                .unwrap_or_default();
            self.context.clear_rect(0.0, 0.0, 1.0, 1.0);
            self.context.set_fill_style_str(&color);
            self.context.fill_rect(0.0, 0.0, 1.0, 1.0);
            let data = self
                .context
                .get_image_data(0.0, 0.0, 1.0, 1.0)
                .map_or_else(|_| Vec::new(), |pixel| pixel.data().to_vec());
            let channel = |at: usize| f32::from(data.get(at).copied().unwrap_or(0)) / 255.0;
            [channel(0), channel(1), channel(2)]
        })
    }
}
