//! The field: one fixed WebGL2 canvas behind the page.
//!
//! A dot lattice whose resolution follows attention: a quadtree of 24, 12 and 6 px cells.
//! Where nothing wants you it rests at 24 px, sparse and slowly breathing. Toward a surface
//! it subdivides, and a Bayer threshold decides each dot. A surface marked `data-want="2"`
//! pulls the finest lattice far out around itself and tints it in the colour of trouble;
//! `data-want="1"` pulls it less far. Hover or focus resolves a surface.
//!
//! The canvas draws at one pixel per CSS pixel and the browser scales it with
//! `image-rendering: pixelated`, so a pixel ratio of 2 looks the same at a quarter of the
//! fill. Without WebGL2 the page keeps the CSS lattice on `<html>`. With `?perf` in the
//! address, a readout shows the field's frame rate and frame time.

use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::js_sys::{Array, Function, Object, Reflect, WeakMap};
use web_sys::{
    AddEventListenerOptions, CustomEvent, Document, DomRect, Element, Event, EventTarget,
    HtmlCanvasElement, MediaQueryList, PointerEvent,
};

mod gpu;
mod motion;
mod perf;
mod sampler;
mod shaders;

use gpu::{Frame, Gpu, put};
use motion::{Mode, Spring, ask, machine};
use sampler::Sampler;
use shaders::{MAX_P, MAX_R, MAX_RIP};

/// Starts the field on the page's `data-module="field"` canvas, and the readout with
/// `?perf`.
///
/// # Errors
///
/// Fails when the page has no document.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let debug = web_sys::UrlSearchParams::new_with_str(&window.location().search()?)?.has("perf");
    let field = match document.query_selector("canvas[data-module~=field]")? {
        Some(canvas) => Field::start(&document, canvas.dyn_into()?)?,
        None => None,
    };
    if debug {
        perf::mount(&document, field.as_ref())?;
    }
    Ok(())
}

/// Which spring eases what.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    Surface(u32),
    SurfaceHeat(u32),
    Palette,
    PaletteHeat,
    Pointer,
    Machine(String),
}

/// A surface the lattice resolves toward, and what it asks: gain, reach, heat.
struct Target {
    keys: (Key, Key),
    rect: DomRect,
    ask: [f32; 3],
}

/// A machine on home's map, as a point the lattice gathers around.
struct Dock {
    name: String,
    x: f64,
    y: f64,
    target: f64,
    sigma: f64,
}

struct Pointer {
    x: f64,
    y: f64,
    inside: bool,
    moved: f64,
}

/// A point the lattice resolves around, with the time it was made.
struct Mark {
    x: f64,
    y: f64,
    at: f64,
}

type Tick = OnceCell<Closure<dyn FnMut(f64)>>;

struct Field {
    gpu: Gpu,
    canvas: HtmlCanvasElement,
    document: Document,
    root: Element,
    reduced: MediaQueryList,
    sampler: Sampler,
    colors: [[f32; 3]; 4],
    ids: WeakMap,
    next_id: u32,
    /// A list, not a map: a few dozen springs, and a map's hasher costs Wasm size.
    springs: Vec<(Key, Spring)>,
    palette: Option<Element>,
    pointer: Pointer,
    trail: VecDeque<Mark>,
    ripples: Vec<Mark>,
    frame: Frame,
    started: f64,
    entrance: f64,
    raf: i32,
    last: f64,
    last_draw: f64,
    dirty: bool,
    drawn: bool,
    paused: bool,
    tick: Tick,
    perf: Option<perf::Readout>,
    /// The window's size in CSS pixels, and CSS pixels per canvas pixel.
    css: (i32, i32),
    scale: u32,
}

type Shared = Rc<RefCell<Field>>;

/// The length of `(x, y)`; `f64::hypot` brings a software `fma` into the Wasm.
fn distance(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

fn listen(target: &EventTarget, kind: &str, listener: impl FnMut(Event) + 'static) {
    let closure = Closure::<dyn FnMut(Event)>::new(listener);
    let options = AddEventListenerOptions::new();
    options.set_passive(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        kind,
        closure.as_ref().unchecked_ref(),
        &options,
    );
    closure.forget();
}

fn elements(list: Result<web_sys::NodeList, JsValue>) -> Vec<Element> {
    let Ok(list) = list else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|index| list.item(index)?.dyn_into().ok())
        .collect()
}

impl Field {
    /// Builds the field and starts its loop, or leaves the CSS lattice when the browser has
    /// no WebGL2 or the shaders do not build.
    fn start(document: &Document, canvas: HtmlCanvasElement) -> Result<Option<Shared>, JsValue> {
        let gpu = match Gpu::new(&canvas) {
            Ok(Some(gpu)) => gpu,
            Ok(None) => return Ok(None),
            Err(error) => {
                web_sys::console::warn_1(&error);
                return Ok(None);
            }
        };
        let window = web_sys::window().ok_or("no window")?;
        let root = document.document_element().ok_or("no root")?;
        let reduced = window
            .match_media("(prefers-reduced-motion: reduce)")?
            .ok_or("no media query")?;
        let started = now();
        let short = root.get_attribute("data-entrance").as_deref() == Some("short");
        let field = Rc::new(RefCell::new(Self {
            gpu,
            canvas,
            document: document.clone(),
            root,
            reduced,
            sampler: Sampler::new(document)?,
            colors: [[0.0; 3]; 4],
            ids: WeakMap::new(),
            next_id: 0,
            springs: Vec::new(),
            // The palette says when it opens, but it may have opened before this bundle started.
            palette: document
                .query_selector("dialog.palette[open]")
                .ok()
                .flatten(),
            pointer: Pointer {
                x: -1e4,
                y: -1e4,
                inside: false,
                moved: 0.0,
            },
            trail: VecDeque::new(),
            ripples: Vec::new(),
            frame: Frame::EMPTY,
            started,
            entrance: if short { 480.0 } else { 1100.0 },
            raf: 0,
            last: started,
            last_draw: 0.0,
            dirty: true,
            drawn: false,
            paused: false,
            tick: OnceCell::new(),
            perf: None,
            css: (1, 1),
            scale: 1,
        }));
        let tick = {
            let field = Rc::clone(&field);
            Closure::<dyn FnMut(f64)>::new(move |time| field.borrow_mut().on_frame(time))
        };
        let _ = field.borrow().tick.set(tick);
        wire(&field, &window, document);
        {
            let mut state = field.borrow_mut();
            state.colors = state.sampler.read();
            state.resize();
            state.wake();
        }
        Ok(Some(field))
    }

    fn mode(&self) -> Mode {
        Mode::new(
            self.reduced.matches(),
            self.root.get_attribute("data-intensity").as_deref(),
        )
    }

    /// One canvas pixel per CSS pixel of the window, or per `scale` CSS pixels each way.
    fn resize(&mut self) {
        let window = web_sys::window();
        let measure = |value: Option<Result<JsValue, JsValue>>| -> u32 {
            let pixels = value
                .and_then(Result::ok)
                .and_then(|value| value.as_f64())
                .unwrap_or(1.0)
                .round()
                .max(1.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a window's size in CSS pixels"
            )]
            let pixels = pixels as u32;
            pixels
        };
        let width = measure(window.as_ref().map(web_sys::Window::inner_width));
        let height = measure(window.as_ref().map(web_sys::Window::inner_height));
        let int = |pixels: u32| i32::try_from(pixels).unwrap_or(i32::MAX);
        self.css = (int(width), int(height));
        let scale = self.scale;
        let (width, height) = (width.div_ceil(scale), height.div_ceil(scale));
        if width != self.canvas.width() || height != self.canvas.height() {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }
        self.gpu
            .resize((int(width), int(height)), self.css, int(scale));
        self.dirty = true;
    }

    fn palette_open(&self) -> bool {
        self.palette
            .as_ref()
            .is_some_and(|palette| palette.has_attribute("open"))
    }

    /// The springs for a surface, by a number kept with the element itself.
    fn keys(&mut self, element: &Element) -> (Key, Key) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "ids are small counts this module wrote"
        )]
        let id = if let Some(id) = self.ids.get(element).as_f64() {
            id as u32
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.ids.set(element, &JsValue::from(id));
            id
        };
        (Key::Surface(id), Key::SurfaceHeat(id))
    }

    /// Surfaces in view, wanting ones first, so the cap never drops them. With the palette
    /// open, only the palette resolves.
    fn targets(&mut self, mode: Mode) -> Vec<Target> {
        let surfaces = elements(self.document.query_selector_all("[data-surface]"));
        let mut out = Vec::new();
        if self.palette_open() {
            if let Some(palette) = &self.palette {
                out.push(Target {
                    keys: (Key::Palette, Key::PaletteHeat),
                    rect: palette.get_bounding_client_rect(),
                    ask: [1.0, 64.0, 0.0],
                });
            }
            for surface in surfaces {
                out.push(Target {
                    keys: self.keys(&surface),
                    rect: surface.get_bounding_client_rect(),
                    ask: [0.0, 34.0, 0.0],
                });
            }
            return out;
        }
        let height = f64::from(self.css.1);
        for surface in surfaces {
            let rect = surface.get_bounding_client_rect();
            if rect.bottom() < -160.0 || rect.top() > height + 160.0 || rect.width() == 0.0 {
                continue;
            }
            let active = surface.matches(":hover, :focus-within").unwrap_or(false);
            let want = surface.get_attribute("data-want");
            let reading = surface.has_attribute("data-reading");
            out.push(Target {
                keys: self.keys(&surface),
                rect,
                ask: ask(want.as_deref(), active, mode.quiet(), reading),
            });
        }
        out.sort_by(|a, b| b.ask[0].total_cmp(&a.ask[0]));
        out
    }

    fn docks(&self) -> Vec<Dock> {
        let Ok(Some(map)) = self.document.query_selector(".map:not(.unknown)") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for dock in elements(map.query_selector_all(".map-svg .dock")) {
            let Ok(Some(dot)) = dock.query_selector("circle") else {
                continue;
            };
            let rect = dot.get_bounding_client_rect();
            if rect.width() == 0.0 {
                continue;
            }
            let name = dock.get_attribute("data-m").unwrap_or_default();
            let quoted = web_sys::css::escape(&name);
            let selector = [
                ".dock-card[data-m=\"",
                &quoted,
                "\"]:is(:hover, :focus-visible), .m[data-m=\"",
                &quoted,
                "\"]:hover",
            ]
            .concat();
            let active = map.query_selector(&selector).ok().flatten().is_some();
            let want = dock
                .get_attribute("data-want")
                .and_then(|want| want.parse().ok())
                .unwrap_or(0);
            let (target, sigma) = machine(want, active);
            out.push(Dock {
                name,
                x: rect.left() + rect.width() / 2.0,
                y: rect.top() + rect.height() / 2.0,
                target,
                sigma,
            });
        }
        out
    }

    /// Moves one spring a frame toward `target`, and returns where it is.
    fn ease(&mut self, key: Key, target: f64, dt: f64, mode: Mode, moving: &mut bool) -> f64 {
        let at = if let Some(at) = self.springs.iter().position(|(known, _)| *known == key) {
            at
        } else {
            self.springs.push((key, Spring::default()));
            self.springs.len() - 1
        };
        let spring = &mut self.springs[at].1;
        *moving |= spring.step(target, dt, mode);
        spring.x
    }

    /// Eases every spring toward this frame's targets and fills the focus pass's inputs.
    /// Says whether anything is still moving.
    fn step(&mut self, dt: f64, time: f64) -> bool {
        let mode = self.mode();
        let mut moving = false;
        self.step_surfaces(mode, dt, &mut moving);
        self.step_points(mode, dt, time, &mut moving);
        self.step_ripples(mode, time, &mut moving);
        moving
    }

    fn step_surfaces(&mut self, mode: Mode, dt: f64, moving: &mut bool) {
        let mut used = 0;
        for target in self.targets(mode).into_iter().take(MAX_R) {
            let rect = &target.rect;
            put(
                &mut self.frame.rects,
                used,
                [rect.left(), rect.top(), rect.right(), rect.bottom()],
            );
            let (key, heat) = target.keys;
            let gain = self.ease(key, f64::from(target.ask[0]), dt, mode, moving);
            let warmth = self.ease(heat, f64::from(target.ask[2]), dt, mode, moving);
            put(
                &mut self.frame.gains,
                used,
                [
                    gain.max(0.0),
                    f64::from(target.ask[1]),
                    warmth.max(0.0),
                    0.0,
                ],
            );
            used += 1;
        }
        self.frame.surfaces = used;
    }

    /// The machines, the pointer and, when lively, the pointer's trail. The machines leave
    /// one point for the pointer.
    fn step_points(&mut self, mode: Mode, dt: f64, time: f64, moving: &mut bool) {
        let mut used = 0;
        for dock in self.docks().into_iter().take(MAX_P - 1) {
            let gain = self.ease(Key::Machine(dock.name), dock.target, dt, mode, moving);
            put(
                &mut self.frame.points,
                used,
                [dock.x, dock.y, gain, dock.sigma],
            );
            used += 1;
        }
        let pointer = &self.pointer;
        let wanted = if !mode.quiet() && pointer.inside && !self.palette_open() {
            if time - pointer.moved > 2600.0 {
                0.25
            } else {
                0.5
            }
        } else {
            0.0
        };
        let gain = self.ease(Key::Pointer, wanted, dt, mode, moving);
        let pointer = &self.pointer;
        if gain > 0.002 && used < MAX_P {
            put(
                &mut self.frame.points,
                used,
                [pointer.x, pointer.y, gain, -150.0],
            );
            used += 1;
        }
        let lively = mode == Mode::Lively;
        if lively
            && pointer.inside
            && self
                .trail
                .back()
                .is_none_or(|last| distance(last.x - pointer.x, last.y - pointer.y) > 26.0)
        {
            self.trail.push_back(Mark {
                x: pointer.x,
                y: pointer.y,
                at: time,
            });
        }
        while self
            .trail
            .front()
            .is_some_and(|first| time - first.at > 900.0 || self.trail.len() > 6)
        {
            self.trail.pop_front();
        }
        if !lively {
            self.trail.clear();
        }
        for mark in &self.trail {
            if used >= MAX_P {
                break;
            }
            let fade = 1.0 - (time - mark.at) / 900.0;
            put(
                &mut self.frame.points,
                used,
                [mark.x, mark.y, 0.42 * fade * fade, -70.0],
            );
            used += 1;
            *moving = true;
        }
        self.frame.points_used = used;
    }

    /// Rings from a press, when lively, newest first.
    fn step_ripples(&mut self, mode: Mode, time: f64, moving: &mut bool) {
        let lively = mode == Mode::Lively;
        self.ripples
            .retain(|ripple| lively && (time - ripple.at) / 1000.0 <= 1.1);
        let mut used = 0;
        for ripple in self.ripples.iter().rev() {
            let age = (time - ripple.at) / 1000.0;
            put(
                &mut self.frame.ripples,
                used,
                [ripple.x, ripple.y, age * 520.0, 0.75 * (1.0 - age / 1.1)],
            );
            used += 1;
            *moving = true;
        }
        self.frame.rings = used;
    }

    /// Pass one, pass two, or both as `which` says, 1, 2 or 3. Kept apart so a bench can
    /// time each. Returns how far the entrance has come, from 0 to 1.
    fn passes(&self, time: f64, which: u32) -> f64 {
        let mode = self.mode();
        let age = time - self.started;
        let reveal = match mode {
            Mode::Still => 1.0,
            Mode::Calm => (age / 320.0).min(1.0),
            _ => (age / self.entrance).min(1.0),
        };
        // A whole number of canvas pixels, so a scaled lattice keeps its dots' size.
        let drift = if mode.quiet() {
            0.0
        } else {
            let scrolled = web_sys::window()
                .and_then(|window| window.scroll_y().ok())
                .unwrap_or_default();
            let scale = f64::from(self.scale);
            (scrolled * 0.3 / scale).round() * scale
        };
        if which & 1 != 0 {
            let t = if mode.quiet() {
                40.0
            } else {
                age / 1000.0 + 40.0
            };
            self.gpu.focus(&self.frame, drift, t);
        }
        if which & 2 != 0 {
            let eased = 1.0 - (1.0 - reveal).powi(3);
            self.gpu.lattice(eased, drift, &self.colors);
        }
        reveal
    }

    /// Draws one frame, and says whether another is needed.
    fn draw(&mut self, time: f64) -> bool {
        let begun = now();
        let dt = ((time - self.last) / 1000.0).min(0.05);
        self.last = time;
        let moving = self.step(dt, time);
        let reveal = self.passes(time, 3);
        let cpu = now() - begun;
        let (mode, size) = (self.mode().name(), self.css);
        if let Some(readout) = &mut self.perf {
            readout.frame(&self.gpu.gl, cpu, time, mode, size);
        }
        if !self.drawn {
            self.drawn = true;
            let _ = self.root.class_list().add_1("gl-on");
        }
        self.dirty = false;
        self.last_draw = time;
        moving || reveal < 1.0
    }

    fn on_frame(&mut self, time: f64) {
        self.raf = 0;
        if self.document.hidden() || self.paused {
            return;
        }
        let breathing = !self.mode().quiet();
        let pointer_live = self.pointer.inside && time - self.pointer.moved < 1200.0;
        // Breathing alone is slow, so it draws at 30 frames a second; anything the reader
        // does draws at the display's rate.
        let busy = if self.dirty
            || pointer_live
            || !self.trail.is_empty()
            || !self.ripples.is_empty()
            || !breathing
            || time - self.last_draw > 30.0
        {
            self.draw(time)
        } else {
            true
        };
        if busy || breathing || self.dirty {
            self.request();
        }
    }

    fn request(&mut self) {
        if let (Some(window), Some(tick)) = (web_sys::window(), self.tick.get()) {
            self.raf = window
                .request_animation_frame(tick.as_ref().unchecked_ref())
                .unwrap_or(0);
        }
    }

    fn wake(&mut self) {
        if self.raf == 0 && !self.document.hidden() && !self.paused {
            self.request();
        }
    }

    fn stop(&mut self) {
        if self.raf != 0
            && let Some(window) = web_sys::window()
        {
            let _ = window.cancel_animation_frame(self.raf);
        }
        self.raf = 0;
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.wake();
    }

    /// For the bench: holds the page's own loop, or lets it run again.
    fn pause(&mut self, on: bool) {
        self.paused = on;
        if on {
            self.stop();
        } else {
            self.wake();
        }
    }

    /// Draws `count` forced frames of the passes `which` names (0 for the read-back alone),
    /// each closed by reading a pixel back, so its time includes rasterising and not only
    /// submitting. Returns each frame's milliseconds.
    fn bench(&mut self, count: u32, which: u32) -> Vec<f64> {
        self.step(0.016, now());
        (0..count)
            .map(|_| {
                let begun = now();
                if which != 0 {
                    self.passes(begun, which);
                }
                self.gpu.finish();
                now() - begun
            })
            .collect()
    }
}

/// The page's events the field follows.
fn wire(field: &Shared, window: &web_sys::Window, document: &Document) {
    let on = |target: &EventTarget, kind: &str, act: fn(&mut Field, &Event)| {
        let field = Rc::clone(field);
        listen(target, kind, move |event| {
            act(&mut field.borrow_mut(), &event);
        });
    };
    on(document, "atrium:palette", |field, event| {
        field.palette = event
            .unchecked_ref::<CustomEvent>()
            .detail()
            .dyn_into()
            .ok();
        field.touch();
    });
    for kind in ["atrium:prefs", "atrium:hour"] {
        on(document, kind, |field, _| {
            field.colors = field.sampler.read();
            field.touch();
        });
    }
    on(window, "pointermove", |field, event| {
        let event = event.unchecked_ref::<PointerEvent>();
        if event.pointer_type() == "touch" {
            return;
        }
        field.pointer = Pointer {
            x: f64::from(event.client_x()),
            y: f64::from(event.client_y()),
            inside: true,
            moved: now(),
        };
        field.wake();
    });
    on(document, "pointerleave", |field, _| {
        field.pointer.inside = false;
        field.wake();
    });
    on(window, "pointerdown", |field, event| {
        if field.mode() != Mode::Lively {
            return;
        }
        let event = event.unchecked_ref::<PointerEvent>();
        field.ripples.push(Mark {
            x: f64::from(event.client_x()),
            y: f64::from(event.client_y()),
            at: now(),
        });
        if field.ripples.len() > MAX_RIP {
            field.ripples.remove(0);
        }
        field.wake();
    });
    on(window, "scroll", |field, _| field.touch());
    on(window, "resize", |field, _| {
        field.resize();
        field.wake();
    });
    for kind in ["pointerover", "pointerout", "focusin", "focusout"] {
        on(document, kind, |field, _| field.touch());
    }
    on(document, "visibilitychange", |field, _| {
        if field.document.hidden() {
            field.stop();
        } else {
            field.last = now();
            field.touch();
        }
    });
    let reduced = field.borrow().reduced.clone();
    on(&reduced, "change", |field, _| field.touch());
    // A lost context keeps the CSS lattice; preventing the default lets it be restored.
    let canvas = field.borrow().canvas.clone();
    let lost = Closure::<dyn FnMut(Event)>::new({
        let field = Rc::clone(field);
        move |event: Event| {
            event.prevent_default();
            let mut field = field.borrow_mut();
            let _ = field.root.class_list().remove_1("gl-on");
            field.stop();
        }
    });
    let _ =
        canvas.add_event_listener_with_callback("webglcontextlost", lost.as_ref().unchecked_ref());
    lost.forget();
}

/// The bench's handle, `window.__field`, only with `?perf`: the context, `pause(on)` and
/// `bench(count, which)`.
fn expose(field: &Shared) -> Result<(), JsValue> {
    let handle = Object::new();
    Reflect::set(&handle, &"gl".into(), &field.borrow().gpu.gl)?;
    let pause = {
        let field = Rc::clone(field);
        Closure::<dyn FnMut(bool)>::new(move |on| field.borrow_mut().pause(on))
    };
    Reflect::set(
        &handle,
        &"pause".into(),
        pause.as_ref().unchecked_ref::<Function>(),
    )?;
    pause.forget();
    let bench = {
        let field = Rc::clone(field);
        Closure::<dyn FnMut(u32, u32) -> Array>::new(move |count, which| {
            field
                .borrow_mut()
                .bench(count, which)
                .into_iter()
                .map(JsValue::from)
                .collect()
        })
    };
    Reflect::set(
        &handle,
        &"bench".into(),
        bench.as_ref().unchecked_ref::<Function>(),
    )?;
    bench.forget();
    let window = web_sys::window().ok_or("no window")?;
    Reflect::set(&window, &"__field".into(), &handle)?;
    Ok(())
}
