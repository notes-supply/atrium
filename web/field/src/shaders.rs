//! The prototype's shaders as they are, with its limits written in.

/// Surfaces, points and ripples the focus pass takes at most.
pub const MAX_R: usize = 16;
pub const MAX_P: usize = 8;
pub const MAX_RIP: usize = 4;

pub const VS: &str = "#version 300 es
void main(){ vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); gl_Position = vec4(p * 2. - 1., 0., 1.); }";

/// Pass one: focus, warmth, breath and heat per 6 px cell, into a small texture, so the
/// costly part runs once per 36 CSS px, not once per pixel.
pub const FOCUS_FS: &str = "#version 300 es
precision highp float;
out vec4 o;
uniform float u_drift6, u_t;
uniform vec4 u_rect[16]; uniform vec4 u_gain[16]; uniform int u_nr;
uniform vec4 u_pt[8]; uniform int u_np;
uniform vec4 u_rip[4]; uniform int u_nrip;
float h21(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p){ vec2 i = floor(p), f = fract(p); f = f * f * (3. - 2. * f);
  return mix(mix(h21(i), h21(i + vec2(1, 0)), f.x), mix(h21(i + vec2(0, 1)), h21(i + vec2(1, 1)), f.x), f.y); }
void main(){
  vec2 q = (floor(gl_FragCoord.xy) + vec2(.5, .5 + floor(u_drift6))) * 6. - vec2(0., u_drift6 * 6.);
  float f = 0., w = 0., heat = 0.;
  for (int i = 0; i < 16; i++) { if (i >= u_nr) break;
    vec4 r = u_rect[i]; vec4 g = u_gain[i]; vec2 c = (r.xy + r.zw) * .5, h = (r.zw - r.xy) * .5;
    vec2 d = abs(q - c) - h; float sd = max(length(max(d, 0.)) + min(max(d.x, d.y), 0.), 0.);
    float k = exp(-sd / g.y);
    f = max(f, g.x * k); heat = max(heat, g.z * exp(-sd / (g.y * .8))); }
  for (int i = 0; i < 8; i++) { if (i >= u_np) break;
    vec4 p = u_pt[i]; vec2 d = q - p.xy; float g = abs(p.z) * exp(-dot(d, d) / (2. * p.w * p.w));
    f = max(f, g); if (p.w < 0.) w = max(w, g); if (p.z < 0.) heat = max(heat, g); }
  for (int i = 0; i < 4; i++) { if (i >= u_nrip) break;
    vec4 r = u_rip[i]; float d = length(q - r.xy) - r.z; float g = r.w * exp(-d * d / 968.);
    f = max(f, g); w = max(w, g); }
  float n = noise((q + vec2(0., u_drift6 * 6.)) * .0038 + vec2(u_t * .027, -u_t * .016));
  o = vec4(min(f, 1.), min(w, 1.), n, min(heat, 1.));
}";

/// Pass two: the lattice. Each pixel finds its cell at 24, 12 or 6 px by the focus there,
/// and the cell's Bayer threshold decides its dot. `u_scale` is CSS pixels per canvas
/// pixel: 1 draws the prototype's pixels, 2 one of every four, each dot then 2 px.
pub const LATTICE_FS: &str = "#version 300 es
precision highp float;
out vec4 o;
uniform vec2 u_res;
uniform float u_breath, u_reveal, u_drift, u_scale;
uniform vec3 u_bg, u_dot, u_warm, u_hot;
uniform sampler2D u_focus;
float b2(vec2 a){ a = floor(a); return fract(dot(a, vec2(.5, a.y * .75))); }
float b4(vec2 a){ return b2(.5 * a) * .25 + b2(a); }
float b8(vec2 a){ return b4(.5 * a) * .25 + b2(a); }
vec4 focusAt(vec2 g){
  ivec2 t = ivec2(floor(g / 6.)) - ivec2(0, int(floor(u_drift / 6.)));
  t = clamp(t, ivec2(0), textureSize(u_focus, 0) - 1);
  return texelFetch(u_focus, t, 0);
}
void main(){
  vec2 q = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y) * u_scale;
  vec2 g = floor(q + vec2(0., u_drift));
  float P = 24., lvl = 0.;
  vec2 c = floor(g / P);
  vec4 fw = focusAt((c + .5) * P);
  if (fw.x * u_reveal > .1 + .2 * b4(c)) {
    P = 12.; lvl = 1.; c = floor(g / P); fw = focusAt((c + .5) * P);
    if (fw.x * u_reveal > .34 + .26 * b4(c)) { P = 6.; lvl = 2.; c = floor(g / P); fw = focusAt((c + .5) * P); }
  }
  float v = (.03 + .26 * fw.z * fw.z * u_breath + fw.x * .86) * u_reveal;
  vec3 col = u_bg;
  if (v > b8(c) + .001) {
    float s = lvl == 0. ? 3. : 2.;
    vec2 local = mod(g, P) - floor(P * .5 - s * .5);
    if (all(greaterThanEqual(local, vec2(0.))) && all(lessThan(local, vec2(s)))) {
      float k = lvl == 0. ? .62 : lvl == 1. ? .82 : 1.;
      vec3 dot = mix(u_dot, u_warm, clamp(fw.y * 1.3, 0., .9));
      dot = mix(dot, u_hot, clamp(fw.w * u_reveal * 1.15, 0., .95));
      col = mix(u_bg, dot, k);
    }
  }
  o = vec4(col, 1.);
}";
