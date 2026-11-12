//! With `?perf` in the address, a readout of the field's frames per second and frame time,
//! measured in the page. CPU is the time to build and submit a frame; GPU is the time the
//! GPU spent on it, where the driver offers a timer query (most desktop GPUs do,
//! `SwiftShader` does not). Then the field's stage, its late time and late frames since it
//! last stepped, and each step down: to what, when, and on how many late frames.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, WebGl2RenderingContext as Gl, WebGlQuery};

use crate::stage::Governor;
use crate::{Shared, expose};

/// `EXT_disjoint_timer_query_webgl2`'s constants.
const TIME_ELAPSED: u32 = 0x88BF;
const GPU_DISJOINT: u32 = 0x8FBB;
/// Frames the readout keeps.
const KEEP: usize = 120;

/// The GPU timer extension, once asked for.
#[derive(Default)]
enum Timer {
    #[default]
    Unasked,
    Missing,
    Present,
}

#[derive(Default)]
struct State {
    out: Option<Element>,
    cpu: VecDeque<f64>,
    gaps: VecDeque<f64>,
    last: f64,
    gpu: Option<f64>,
    query: Option<WebGlQuery>,
    timer: Timer,
    painting: bool,
    mode: &'static str,
    size: (i32, i32),
    governor: Option<Governor>,
}

/// The readout's half that the field feeds every frame.
#[derive(Default, Clone)]
pub struct Readout(Rc<RefCell<State>>);

pub fn mount(document: &Document, field: Option<&Shared>) -> Result<(), JsValue> {
    let out = document.create_element("pre")?;
    out.set_class_name("perf");
    out.set_attribute("aria-live", "off")?;
    document.body().ok_or("no body")?.append_child(&out)?;
    let readout = match field {
        Some(field) => {
            expose(field)?;
            let mut field = field.borrow_mut();
            field.perf.get_or_insert_with(Readout::default).clone()
        }
        None => Readout::default(),
    };
    let mut state = readout.0.borrow_mut();
    state.out = Some(out);
    state.mode = "off";
    if let Some(field) = field {
        let field = field.borrow();
        state.mode = field.mode().name();
        state.size = field.css;
        state.governor = Some(field.governor.clone());
    }
    state.paint();
    Ok(())
}

impl Readout {
    pub fn frame(
        &mut self,
        gl: &Gl,
        cpu: f64,
        time: f64,
        mode: &'static str,
        size: (i32, i32),
        governor: &Governor,
    ) {
        let mut state = self.0.borrow_mut();
        push(&mut state.cpu, cpu);
        if state.last > 0.0 {
            let gap = time - state.last;
            push(&mut state.gaps, gap);
        }
        state.last = time;
        state.mode = mode;
        state.size = size;
        state.governor = Some(governor.clone());
        state.time_gpu(gl);
        if !state.painting && state.out.is_some() {
            state.painting = true;
            let shared = Rc::clone(&self.0);
            let paint = Closure::once_into_js(move || shared.borrow_mut().paint());
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    paint.unchecked_ref(),
                    250,
                );
            }
        }
    }
}

fn push(values: &mut VecDeque<f64>, value: f64) {
    values.push_back(value);
    if values.len() > KEEP {
        values.pop_front();
    }
}

impl State {
    /// Keeps one timer query in flight: begun at a frame, ended at the next animation
    /// frame, read once the GPU has its answer.
    fn time_gpu(&mut self, gl: &Gl) {
        if let Timer::Unasked = self.timer {
            let present = gl
                .get_extension("EXT_disjoint_timer_query_webgl2")
                .ok()
                .flatten()
                .is_some();
            self.timer = if present {
                Timer::Present
            } else {
                Timer::Missing
            };
        }
        if !matches!(self.timer, Timer::Present) {
            return;
        }
        if let Some(query) = &self.query
            && gl
                .get_query_parameter(query, Gl::QUERY_RESULT_AVAILABLE)
                .as_bool()
                == Some(true)
        {
            let disjoint = gl
                .get_parameter(GPU_DISJOINT)
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            if !disjoint {
                self.gpu = gl
                    .get_query_parameter(query, Gl::QUERY_RESULT)
                    .as_f64()
                    .map(|ns| ns / 1e6);
            }
            gl.delete_query(Some(query));
            self.query = None;
        }
        if self.query.is_none()
            && let Some(query) = gl.create_query()
        {
            gl.begin_query(TIME_ELAPSED, &query);
            self.query = Some(query);
            let gl = gl.clone();
            let end = Closure::once_into_js(move || gl.end_query(TIME_ELAPSED));
            if let Some(window) = web_sys::window() {
                let _ = window.request_animation_frame(end.unchecked_ref());
            }
        }
    }

    fn paint(&mut self) {
        self.painting = false;
        let Some(out) = &self.out else {
            return;
        };
        let fps = if self.gaps.is_empty() {
            0.0
        } else {
            #[expect(clippy::cast_precision_loss, reason = "at most 120 frames")]
            let count = self.gaps.len() as f64;
            1000.0 / (self.gaps.iter().sum::<f64>() / count)
        };
        let gpu = match (&self.timer, self.gpu) {
            (_, Some(ms)) => [fixed(ms, 2).as_str(), " ms"].concat(),
            (Timer::Missing, None) => "no timer query".to_owned(),
            _ => "waiting".to_owned(),
        };
        let fps = fixed(fps, 0);
        let text = [
            "field ",
            self.mode,
            "  ",
            &size(self.size.0),
            "x",
            &size(self.size.1),
            " css px\n",
            &" ".repeat(3_usize.saturating_sub(fps.len())),
            &fps,
            " fps   gap p90 ",
            &fixed(percentile(&self.gaps, 0.9), 1),
            " ms\ncpu  med ",
            &fixed(percentile(&self.cpu, 0.5), 2),
            "  p90 ",
            &fixed(percentile(&self.cpu, 0.9), 2),
            " ms\ngpu  ",
            &gpu,
            &self.governor.as_ref().map_or_else(String::new, stage),
        ]
        .concat();
        out.set_text_content(Some(&text));
    }
}

/// The stage's lines: `stage half  late 40 ms, 2 of 31 frames`, then one line for each
/// step down.
fn stage(governor: &Governor) -> String {
    let (late, of) = governor.late();
    let mut text = [
        "\nstage ",
        governor.stage.name(),
        if governor.held { ", held" } else { "" },
        "  late ",
        &fixed(governor.owed, 0),
        " ms, ",
        &digits(late.into()),
        " of ",
        &digits(of.into()),
        " frames",
    ]
    .concat();
    for step in &governor.steps {
        text += &[
            "\nstep  ",
            step.to.name(),
            " at ",
            &fixed(step.at / 1000.0, 2),
            " s, ",
            &digits(step.late.into()),
            " of ",
            &digits(step.of.into()),
            " frames late",
        ]
        .concat();
    }
    text
}

fn size(pixels: i32) -> String {
    digits(u64::try_from(pixels).unwrap_or(0))
}

/// The value below which a share `p` of `values` falls, as the prototype counts it.
fn percentile(values: &VecDeque<f64>, p: f64) -> f64 {
    let mut sorted: Vec<f64> = values.iter().copied().collect();
    if sorted.is_empty() {
        return 0.0;
    }
    sorted.sort_by(f64::total_cmp);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "an index into at most 120 frames"
    )]
    let at = ((sorted.len() as f64 * p).floor() as usize).min(sorted.len() - 1);
    sorted[at]
}

/// The decimal digits of `number`, without the formatter, which costs Wasm size.
fn digits(mut number: u64) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'0' + u8::try_from(number % 10).unwrap_or(0));
        number /= 10;
        if number == 0 {
            break;
        }
    }
    out.iter().rev().map(|&digit| char::from(digit)).collect()
}

/// `value` with `decimals` decimals, as `toFixed` writes it, without the float formatter.
fn fixed(value: f64, decimals: u32) -> String {
    let scale = 10_u64.pow(decimals);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "readout values are small and positive"
    )]
    let scaled = (value.max(0.0) * scale as f64).round() as u64;
    let whole = digits(scaled / scale);
    if decimals == 0 {
        return whole;
    }
    let fraction = digits(scaled % scale);
    let zeros = "0".repeat(decimals as usize - fraction.len());
    [whole.as_str(), ".", &zeros, &fraction].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_without_the_formatter() {
        assert_eq!(digits(0), "0");
        assert_eq!(digits(1440), "1440");
    }

    #[test]
    fn fixed_rounds_like_to_fixed() {
        assert_eq!(fixed(21.456, 2), "21.46");
        assert_eq!(fixed(3.04, 1), "3.0");
        assert_eq!(fixed(59.6, 0), "60");
        assert_eq!(fixed(0.004, 2), "0.00");
    }

    #[test]
    fn the_stage_says_why_it_stepped() {
        let mut governor = Governor::new(false);
        assert_eq!(stage(&governor), "\nstage full  late 0 ms, 0 of 0 frames");
        governor.record(false, 50.0);
        for frame in 1..=4 {
            governor.record(true, 50.0 + f64::from(frame) * 100.0);
        }
        governor.record(true, 600.0);
        assert_eq!(
            stage(&governor),
            "\nstage half  late 0 ms, 0 of 0 frames\nstep  half at 0.60 s, 5 of 6 frames late"
        );
        let mut held = Governor::new(true);
        held.record(true, 40.0);
        assert_eq!(
            stage(&held),
            "\nstage full, held  late 40 ms, 1 of 1 frames"
        );
    }

    #[test]
    fn percentiles_index_the_sorted_frames() {
        let frames: VecDeque<f64> = (1..=10).map(f64::from).rev().collect();
        assert!((percentile(&frames, 0.5) - 6.0).abs() < f64::EPSILON);
        assert!((percentile(&frames, 0.9) - 10.0).abs() < f64::EPSILON);
        assert!(percentile(&VecDeque::new(), 0.9).abs() < f64::EPSILON);
    }
}
