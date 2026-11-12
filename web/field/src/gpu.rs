//! The WebGL2 side: the context, the two programs and the focus texture between them.

use wasm_bindgen::prelude::*;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as Gl, WebGlContextAttributes, WebGlFramebuffer,
    WebGlPowerPreference, WebGlProgram, WebGlShader, WebGlTexture, WebGlUniformLocation,
};

use crate::shaders::{self, MAX_P, MAX_R, MAX_RIP};

/// What the focus pass draws from: the surfaces, points and ripples of one frame.
pub struct Frame {
    pub rects: [f32; MAX_R * 4],
    pub gains: [f32; MAX_R * 4],
    pub points: [f32; MAX_P * 4],
    pub ripples: [f32; MAX_RIP * 4],
    pub surfaces: usize,
    pub points_used: usize,
    pub rings: usize,
}

impl Frame {
    pub const EMPTY: Self = Self {
        rects: [0.0; MAX_R * 4],
        gains: [0.0; MAX_R * 4],
        points: [0.0; MAX_P * 4],
        ripples: [0.0; MAX_RIP * 4],
        surfaces: 0,
        points_used: 0,
        rings: 0,
    };
}

/// Writes four values into the `index`th vec4 of a uniform array.
#[expect(clippy::cast_possible_truncation, reason = "uniforms are f32")]
pub fn put(slots: &mut [f32], index: usize, values: [f64; 4]) {
    slots[index * 4..index * 4 + 4].copy_from_slice(&values.map(|value| value as f32));
}

fn count(used: usize) -> i32 {
    i32::try_from(used).unwrap_or(0)
}

struct FocusUniforms {
    drift6: Option<WebGlUniformLocation>,
    t: Option<WebGlUniformLocation>,
    rect: Option<WebGlUniformLocation>,
    gain: Option<WebGlUniformLocation>,
    nr: Option<WebGlUniformLocation>,
    pt: Option<WebGlUniformLocation>,
    np: Option<WebGlUniformLocation>,
    rip: Option<WebGlUniformLocation>,
    nrip: Option<WebGlUniformLocation>,
}

struct LatticeUniforms {
    res: Option<WebGlUniformLocation>,
    breath: Option<WebGlUniformLocation>,
    reveal: Option<WebGlUniformLocation>,
    drift: Option<WebGlUniformLocation>,
    scale: Option<WebGlUniformLocation>,
    bg: Option<WebGlUniformLocation>,
    dot: Option<WebGlUniformLocation>,
    warm: Option<WebGlUniformLocation>,
    hot: Option<WebGlUniformLocation>,
    focus: Option<WebGlUniformLocation>,
}

pub struct Gpu {
    pub gl: Gl,
    focus_program: WebGlProgram,
    focus: FocusUniforms,
    lattice_program: WebGlProgram,
    lattice: LatticeUniforms,
    texture: WebGlTexture,
    framebuffer: WebGlFramebuffer,
    /// The canvas's size in its own pixels, and CSS pixels per canvas pixel.
    size: (i32, i32),
    scale: i32,
    /// The focus texture's size: one texel per 6 CSS px cell, and a margin for the drift.
    cells: (i32, i32),
}

fn compile(gl: &Gl, kind: u32, source: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl.create_shader(kind).ok_or("no shader")?;
    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);
    if gl
        .get_shader_parameter(&shader, Gl::COMPILE_STATUS)
        .as_bool()
        != Some(true)
    {
        return Err(gl.get_shader_info_log(&shader).unwrap_or_default().into());
    }
    Ok(shader)
}

fn link(gl: &Gl, fragment: &str) -> Result<WebGlProgram, JsValue> {
    let program = gl.create_program().ok_or("no program")?;
    gl.attach_shader(&program, &compile(gl, Gl::VERTEX_SHADER, shaders::VS)?);
    gl.attach_shader(&program, &compile(gl, Gl::FRAGMENT_SHADER, fragment)?);
    gl.link_program(&program);
    if gl
        .get_program_parameter(&program, Gl::LINK_STATUS)
        .as_bool()
        != Some(true)
    {
        return Err(gl.get_program_info_log(&program).unwrap_or_default().into());
    }
    Ok(program)
}

/// A GL enum as the `i32` some calls take.
fn int(value: u32) -> i32 {
    i32::try_from(value).unwrap_or_default()
}

impl Gpu {
    /// The canvas's WebGL2 context with both programs built, or nothing when the browser has
    /// no WebGL2. A shader that does not build is an error.
    pub fn new(canvas: &HtmlCanvasElement) -> Result<Option<Self>, JsValue> {
        let attributes = WebGlContextAttributes::new();
        attributes.set_antialias(false);
        attributes.set_alpha(false);
        attributes.set_depth(false);
        attributes.set_stencil(false);
        attributes.set_preserve_drawing_buffer(false);
        attributes.set_power_preference(WebGlPowerPreference::LowPower);
        let Some(gl) = canvas.get_context_with_context_options("webgl2", &attributes)? else {
            return Ok(None);
        };
        let gl: Gl = gl.dyn_into()?;
        let focus_program = link(&gl, shaders::FOCUS_FS)?;
        let lattice_program = link(&gl, shaders::LATTICE_FS)?;
        let uniform = |program: &WebGlProgram, name: &str| gl.get_uniform_location(program, name);
        let focus = FocusUniforms {
            drift6: uniform(&focus_program, "u_drift6"),
            t: uniform(&focus_program, "u_t"),
            rect: uniform(&focus_program, "u_rect"),
            gain: uniform(&focus_program, "u_gain"),
            nr: uniform(&focus_program, "u_nr"),
            pt: uniform(&focus_program, "u_pt"),
            np: uniform(&focus_program, "u_np"),
            rip: uniform(&focus_program, "u_rip"),
            nrip: uniform(&focus_program, "u_nrip"),
        };
        let lattice = LatticeUniforms {
            res: uniform(&lattice_program, "u_res"),
            breath: uniform(&lattice_program, "u_breath"),
            reveal: uniform(&lattice_program, "u_reveal"),
            drift: uniform(&lattice_program, "u_drift"),
            scale: uniform(&lattice_program, "u_scale"),
            bg: uniform(&lattice_program, "u_bg"),
            dot: uniform(&lattice_program, "u_dot"),
            warm: uniform(&lattice_program, "u_warm"),
            hot: uniform(&lattice_program, "u_hot"),
            focus: uniform(&lattice_program, "u_focus"),
        };
        gl.bind_vertex_array(gl.create_vertex_array().as_ref());
        let texture = gl.create_texture().ok_or("no texture")?;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&texture));
        for (name, value) in [
            (Gl::TEXTURE_MIN_FILTER, Gl::NEAREST),
            (Gl::TEXTURE_MAG_FILTER, Gl::NEAREST),
            (Gl::TEXTURE_WRAP_S, Gl::CLAMP_TO_EDGE),
            (Gl::TEXTURE_WRAP_T, Gl::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameteri(Gl::TEXTURE_2D, name, int(value));
        }
        let framebuffer = gl.create_framebuffer().ok_or("no framebuffer")?;
        Ok(Some(Self {
            gl,
            focus_program,
            focus,
            lattice_program,
            lattice,
            texture,
            framebuffer,
            size: (0, 0),
            scale: 1,
            cells: (0, 0),
        }))
    }

    /// Sizes the focus texture for a window of `css` pixels, drawn on a canvas of `size`
    /// pixels at `scale` CSS pixels each.
    pub fn resize(&mut self, size: (i32, i32), css: (i32, i32), scale: i32) {
        self.size = size;
        self.scale = scale;
        self.cells = ((css.0 + 5) / 6 + 1, (css.1 + 5) / 6 + 2);
        let gl = &self.gl;
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
        let _ = gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            Gl::TEXTURE_2D,
            0,
            int(Gl::RGBA8),
            self.cells.0,
            self.cells.1,
            0,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            None,
        );
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&self.framebuffer));
        gl.framebuffer_texture_2d(
            Gl::FRAMEBUFFER,
            Gl::COLOR_ATTACHMENT0,
            Gl::TEXTURE_2D,
            Some(&self.texture),
            0,
        );
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
    }

    /// Pass one: the focus per 6 px cell, into the texture.
    #[expect(clippy::cast_possible_truncation, reason = "uniforms are f32")]
    pub fn focus(&self, frame: &Frame, drift: f64, t: f64) {
        let (gl, u) = (&self.gl, &self.focus);
        gl.use_program(Some(&self.focus_program));
        gl.bind_framebuffer(Gl::FRAMEBUFFER, Some(&self.framebuffer));
        gl.viewport(0, 0, self.cells.0, self.cells.1);
        gl.uniform1f(u.drift6.as_ref(), (drift / 6.0) as f32);
        gl.uniform1f(u.t.as_ref(), t as f32);
        gl.uniform4fv_with_f32_array(u.rect.as_ref(), &frame.rects);
        gl.uniform4fv_with_f32_array(u.gain.as_ref(), &frame.gains);
        gl.uniform1i(u.nr.as_ref(), count(frame.surfaces));
        gl.uniform4fv_with_f32_array(u.pt.as_ref(), &frame.points);
        gl.uniform1i(u.np.as_ref(), count(frame.points_used));
        gl.uniform4fv_with_f32_array(u.rip.as_ref(), &frame.ripples);
        gl.uniform1i(u.nrip.as_ref(), count(frame.rings));
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
        gl.bind_framebuffer(Gl::FRAMEBUFFER, None);
    }

    /// Pass two: the lattice, every pixel, in ground, dot, warm and hot.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "uniforms are f32"
    )]
    pub fn lattice(&self, reveal: f64, drift: f64, colors: &[[f32; 3]; 4]) {
        let (gl, u) = (&self.gl, &self.lattice);
        gl.viewport(0, 0, self.size.0, self.size.1);
        gl.use_program(Some(&self.lattice_program));
        gl.active_texture(Gl::TEXTURE0);
        gl.bind_texture(Gl::TEXTURE_2D, Some(&self.texture));
        gl.uniform1i(u.focus.as_ref(), 0);
        gl.uniform2f(u.res.as_ref(), self.size.0 as f32, self.size.1 as f32);
        gl.uniform1f(u.breath.as_ref(), 1.0);
        gl.uniform1f(u.reveal.as_ref(), reveal as f32);
        gl.uniform1f(u.drift.as_ref(), drift as f32);
        gl.uniform1f(u.scale.as_ref(), self.scale as f32);
        let [bg, dot, warm, hot] = colors;
        gl.uniform3fv_with_f32_array(u.bg.as_ref(), bg);
        gl.uniform3fv_with_f32_array(u.dot.as_ref(), dot);
        gl.uniform3fv_with_f32_array(u.warm.as_ref(), warm);
        gl.uniform3fv_with_f32_array(u.hot.as_ref(), hot);
        gl.draw_arrays(Gl::TRIANGLES, 0, 3);
    }

    /// Waits for the GPU by reading one pixel back.
    pub fn finish(&self) {
        let mut pixel = [0_u8; 4];
        let _ = self.gl.read_pixels_with_opt_u8_array(
            0,
            0,
            1,
            1,
            Gl::RGBA,
            Gl::UNSIGNED_BYTE,
            Some(&mut pixel),
        );
    }
}
