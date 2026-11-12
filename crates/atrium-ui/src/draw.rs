//! Drawings made from code, as SVG the pages carry as they are: the map of the wiki, each place
//! on its own, the week of versions, a page's 90 days, and the resolution mark. Every colour is
//! a CSS class, so the theme, the hour and focus stay in the stylesheet.
//!
//! The geometry is isometric boxes on a grid, projected and rounded to a tenth of a unit. A
//! drawing of a place is chosen by its name, so the stylesheet can colour each one.

use std::f64::consts::PI;
use std::fmt::Write;

use maud::{Markup, PreEscaped, html};

/// The 4x4 ordered-dither matrix, row by row.
pub const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// How much a place wants you: at rest, worth a look, or needs you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Want {
    #[default]
    Rest,
    Look,
    Needs,
}

impl Want {
    /// The level as the stylesheet reads it in `data-want`.
    #[must_use]
    pub const fn level(self) -> &'static str {
        match self {
            Self::Rest => "0",
            Self::Look => "1",
            Self::Needs => "2",
        }
    }
}

/// A drawn place on the map. Machines with other names have no drawing of their own and take
/// the first free one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The tall machine on the left: a workstation with a folder on top.
    Ironbark,
    /// The cluster that holds the record.
    Laurel,
    /// The low machine on the right, with a folder.
    Willow,
}

impl Place {
    /// The places in the order the map's cards list them.
    pub const ALL: [Self; 3] = [Self::Ironbark, Self::Laurel, Self::Willow];

    /// The name the stylesheet colours it by, in `data-m`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ironbark => "ironbark",
            Self::Laurel => "laurel",
            Self::Willow => "willow",
        }
    }
}

/// Rounds to a tenth the way the prototype's JavaScript did, half up, and never prints `-0`.
fn r1(n: f64) -> f64 {
    (n * 10.0 + 0.5).floor() / 10.0 + 0.0
}

/// A small count as a coordinate.
fn unit(count: usize) -> f64 {
    u32::try_from(count).map_or(0.0, f64::from)
}

fn escaped(text: &str) -> String {
    html! { (text) }.into_string()
}

/// A 4x4 ordered-dither tile at `k` sixteenths, cells of `c` units, on an opaque ground so a
/// dithered face still hides what is behind it.
fn pattern(out: &mut String, id: &str, k: u8, c: f64, dots: &str, ground: Option<&str>) {
    let size = 4.0 * c;
    let _ = write!(
        out,
        r#"<pattern id="{id}" width="{size}" height="{size}" patternUnits="userSpaceOnUse">"#
    );
    if let Some(ground) = ground {
        let _ = write!(
            out,
            r#"<rect class="{ground}" width="{size}" height="{size}"/>"#
        );
    }
    let _ = write!(out, r#"<g class="{dots}">"#);
    for (i, bayer) in BAYER.iter().enumerate() {
        if *bayer < k {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{c}" height="{c}"/>"#,
                unit(i % 4) * c,
                unit(i / 4) * c
            );
        }
    }
    out.push_str("</g></pattern>");
}

// ── isometric boxes ─────────────────────────────────────────────────────

type Point = [f64; 3];

const SCALE: f64 = 30.0;

fn iso([x, y, z]: Point) -> (f64, f64) {
    let c30 = (PI / 6.0).cos();
    (
        (x - y) * c30 * SCALE,
        f64::midpoint(x, y) * SCALE - z * SCALE,
    )
}

#[derive(Clone, Copy)]
enum Face {
    Top,
    Left,
    Right,
}

impl Face {
    /// Back to front, as the painter draws them.
    const PAINTED: [Self; 3] = [Self::Left, Self::Right, Self::Top];

    const fn letter(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Left => "y",
            Self::Right => "x",
        }
    }
}

/// A detail painted on one face: `u` across it, `v` up or along it.
struct Detail {
    face: Face,
    u0: f64,
    v0: f64,
    u1: f64,
    v1: f64,
    class: &'static str,
}

const fn mark(face: Face, u0: f64, v0: f64, u1: f64, v1: f64, class: &'static str) -> Detail {
    Detail {
        face,
        u0,
        v0,
        u1,
        v1,
        class,
    }
}

struct Cuboid {
    at: Point,
    size: Point,
    tone: &'static str,
    marks: Vec<Detail>,
}

fn cuboid(at: Point, size: Point, tone: &'static str, marks: Vec<Detail>) -> Cuboid {
    Cuboid {
        at,
        size,
        tone,
        marks,
    }
}

impl Cuboid {
    fn far(&self) -> Point {
        [
            self.at[0] + self.size[0],
            self.at[1] + self.size[1],
            self.at[2] + self.size[2],
        ]
    }

    fn face(&self, face: Face) -> [Point; 4] {
        let [x0, y0, z0] = self.at;
        let [x1, y1, z1] = self.far();
        match face {
            Face::Top => [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            Face::Left => [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
            Face::Right => [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
        }
    }

    fn mark(&self, m: &Detail) -> [Point; 4] {
        let [x0, y0, z0] = self.at;
        let [x1, y1, z1] = self.far();
        let e = 0.01;
        match m.face {
            Face::Left => [
                [x0 + m.u0, y1 + e, z0 + m.v0],
                [x0 + m.u1, y1 + e, z0 + m.v0],
                [x0 + m.u1, y1 + e, z0 + m.v1],
                [x0 + m.u0, y1 + e, z0 + m.v1],
            ],
            Face::Right => [
                [x1 + e, y0 + m.u0, z0 + m.v0],
                [x1 + e, y0 + m.u1, z0 + m.v0],
                [x1 + e, y0 + m.u1, z0 + m.v1],
                [x1 + e, y0 + m.u0, z0 + m.v1],
            ],
            Face::Top => [
                [x0 + m.u0, y0 + m.v0, z1 + e],
                [x0 + m.u1, y0 + m.v0, z1 + e],
                [x0 + m.u1, y0 + m.v1, z1 + e],
                [x0 + m.u0, y0 + m.v1, z1 + e],
            ],
        }
    }
}

/// A folder: a stack of sheets, the copy of the wiki a machine holds.
fn folder(out: &mut Vec<Cuboid>, x: f64, y: f64, z: f64, sheets: u8) {
    for i in 0..sheets {
        let i = f64::from(i);
        out.push(cuboid(
            [x + (i % 2.0) * 0.05, y - (i % 3.0) * 0.04, z + i * 0.07],
            [0.9, 1.15, 0.05],
            "pp",
            Vec::new(),
        ));
    }
}

/// Each place's boxes. Painter order is the order of the list.
fn boxes(place: Place) -> Vec<Cuboid> {
    let mut out = Vec::new();
    match place {
        Place::Laurel => {
            for i in 0..3 {
                out.push(cuboid(
                    [5.0, 0.7, f64::from(i) * 0.86],
                    [2.6, 1.9, 0.74],
                    "ap",
                    vec![
                        mark(Face::Left, 0.2, 0.22, 2.0, 0.52, "mk-panel"),
                        mark(Face::Right, 1.45, 0.3, 1.6, 0.44, "mk-led"),
                    ],
                ));
            }
            // the record: a bound ledger on top
            out.push(cuboid(
                [5.55, 1.05, 2.58],
                [1.5, 1.2, 0.26],
                "rc",
                vec![mark(Face::Left, 0.1, 0.06, 1.4, 0.2, "mk-spine")],
            ));
        }
        Place::Ironbark => {
            let mut marks = vec![mark(Face::Right, 0.2, 1.7, 1.75, 1.82, "mk-strip")];
            for i in 0..5 {
                let i = f64::from(i);
                marks.push(mark(
                    Face::Right,
                    0.3,
                    0.3 + i * 0.16,
                    1.1,
                    0.36 + i * 0.16,
                    "mk-vent",
                ));
            }
            marks.push(mark(Face::Left, 1.5, 1.95, 1.7, 2.1, "mk-led"));
            out.push(cuboid([1.3, 5.4, 0.0], [1.9, 2.0, 2.3], "bb", marks));
            folder(&mut out, 1.75, 5.85, 2.3, 5);
        }
        Place::Willow => {
            out.push(cuboid(
                [9.5, 5.1, 0.0],
                [2.2, 2.0, 0.9],
                "ae",
                vec![
                    mark(Face::Right, 0.25, 0.55, 1.75, 0.66, "mk-strip"),
                    mark(Face::Left, 1.8, 0.5, 2.0, 0.66, "mk-led"),
                ],
            ));
            folder(&mut out, 10.1, 5.5, 0.9, 4);
        }
    }
    out
}

const TONES: [&str; 6] = ["st", "bb", "ap", "ae", "rc", "pp"];

/// The cable from each machine with a folder to laurel, on the floor.
fn cable(place: Place) -> Option<[[f64; 2]; 4]> {
    match place {
        Place::Ironbark => Some([[3.2, 6.4], [4.4, 6.4], [4.4, 2.6], [5.0, 2.6]]),
        Place::Willow => Some([[9.5, 6.0], [8.6, 6.0], [8.6, 1.9], [7.6, 1.9]]),
        Place::Laurel => None,
    }
}

fn pattern_defs(prefix: &str) -> String {
    // two levels of resolution per tone and face, rest and half; solid is a fill
    let mut out = String::new();
    for tone in TONES {
        for face in [Face::Top, Face::Left, Face::Right] {
            let (rest, half) = match face {
                Face::Top => (2, 7),
                Face::Left => (3, 9),
                Face::Right => (4, 11),
            };
            let f = face.letter();
            let dots = format!("pd pd-{tone}");
            pattern(
                &mut out,
                &format!("{prefix}r-{tone}-{f}"),
                rest,
                1.5,
                &dots,
                Some("pg"),
            );
            pattern(
                &mut out,
                &format!("{prefix}h-{tone}-{f}"),
                half,
                1.5,
                &dots,
                Some("pg"),
            );
        }
    }
    out
}

fn points(poly: &[Point], project: &impl Fn(Point) -> (f64, f64)) -> String {
    poly.iter()
        .map(|point| {
            let (x, y) = project(*point);
            format!("{x},{y}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_boxes(out: &mut String, cuboids: &[Cuboid], project: &impl Fn(Point) -> (f64, f64)) {
    let prefix = "mp-";
    for b in cuboids {
        out.push_str(r#"<g class="bx">"#);
        for face in Face::PAINTED {
            let (tone, f, pts) = (b.tone, face.letter(), points(&b.face(face), project));
            let _ = write!(
                out,
                r#"<polygon class="dz" fill="url(#{prefix}r-{tone}-{f})" points="{pts}"/><polygon class="dh" fill="url(#{prefix}h-{tone}-{f})" points="{pts}"/><polygon class="sd sd-{tone}-{f}" points="{pts}"/>"#
            );
        }
        for m in &b.marks {
            let _ = write!(
                out,
                r#"<polygon class="mk {}" points="{}"/>"#,
                m.class,
                points(&b.mark(m), project)
            );
        }
        for face in Face::PAINTED {
            let _ = write!(
                out,
                r#"<polygon class="ln" points="{}"/>"#,
                points(&b.face(face), project)
            );
        }
        out.push_str("</g>");
    }
}

struct Bounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

fn bounds<'a>(cuboids: impl IntoIterator<Item = &'a Cuboid>) -> Bounds {
    let mut b = Bounds {
        min_x: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        min_y: f64::INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for cuboid in cuboids {
        for face in [Face::Top, Face::Left, Face::Right] {
            for point in cuboid.face(face) {
                let (x, y) = iso(point);
                b.min_x = b.min_x.min(x);
                b.max_x = b.max_x.max(x);
                b.min_y = b.min_y.min(y);
                b.max_y = b.max_y.max(y);
            }
        }
    }
    b
}

/// The dither patterns every drawing of a place uses, once per page, first in the body.
#[must_use]
pub fn place_defs() -> Markup {
    PreEscaped(format!(
        r#"<svg class="defs" width="0" height="0" aria-hidden="true" focusable="false"><defs>{}</defs></svg>"#,
        pattern_defs("mp-")
    ))
}

const MAP_WIDTH: f64 = 760.0;

fn floor() -> Cuboid {
    cuboid([0.0, 0.0, -0.3], [13.0, 8.6, 0.3], "st", Vec::new())
}

struct MapFrame {
    ox: f64,
    oy: f64,
    height: f64,
}

fn map_frame() -> MapFrame {
    let floor = floor();
    let places: Vec<Cuboid> = Place::ALL.into_iter().flat_map(boxes).collect();
    let b = bounds(std::iter::once(&floor).chain(&places));
    let (top, base) = (12.0, 70.0);
    MapFrame {
        ox: MAP_WIDTH / 2.0 - f64::midpoint(b.max_x, b.min_x),
        oy: top - b.min_y,
        height: (b.max_y - b.min_y + top + base).ceil(),
    }
}

/// The map's width and height in its own units.
#[must_use]
pub fn map_size() -> (f64, f64) {
    (MAP_WIDTH, map_frame().height)
}

/// The map of the wiki: the floor, the places with the want each has, the cables between
/// them, and a leader from each place down to its card below. `title` is what a screen
/// reader hears for the whole drawing.
#[must_use]
pub fn map(want: impl Fn(Place) -> Want, title: &str) -> Markup {
    let MapFrame { ox, oy, height } = map_frame();
    let project = |point: Point| {
        let (a, c) = iso(point);
        (r1(a + ox), r1(c + oy))
    };
    let mut groups: Vec<String> = Vec::new();
    let floor = floor();
    groups.push(r#"<g class="room">"#.to_owned());
    for face in Face::PAINTED {
        groups.push(format!(
            r#"<polygon class="fl fl-{}" points="{}"/>"#,
            face.letter(),
            points(&floor.face(face), &project)
        ));
    }
    let mut line = |from: Point, to: Point| {
        let ((x1, y1), (x2, y2)) = (project(from), project(to));
        groups.push(format!(
            r#"<line class="score" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"/>"#
        ));
    };
    for i in 1..13 {
        let i = f64::from(i);
        line([i, 0.0, 0.0], [i, 8.6, 0.0]);
    }
    for j in 1..9 {
        let j = f64::from(j);
        line([0.0, j, 0.0], [13.0, j, 0.0]);
    }
    groups.push("</g>".to_owned());
    for place in [Place::Ironbark, Place::Willow] {
        let Some(path) = cable(place) else { continue };
        let d = path
            .iter()
            .map(|[x, y]| {
                let (px, py) = project([*x, *y, 0.005]);
                format!("{px} {py}")
            })
            .collect::<Vec<_>>()
            .join(" L");
        groups.push(format!(
            r#"<g class="cable" data-m="{}" data-want="{}"><path class="cb" d="M{d}"/><path class="pk" d="M{d}" pathLength="100"/></g>"#,
            place.name(),
            want(place).level()
        ));
    }
    for place in [Place::Laurel, Place::Ironbark, Place::Willow] {
        let mut group = format!(
            r#"<g class="m" data-m="{}" data-want="{}">"#,
            place.name(),
            want(place).level()
        );
        draw_boxes(&mut group, &boxes(place), &project);
        group.push_str("</g>");
        groups.push(group);
    }
    for (place, from, card, lane) in [
        (
            Place::Ironbark,
            [2.25, 7.4, 0.9],
            MAP_WIDTH / 6.0,
            height - 34.0,
        ),
        (
            Place::Laurel,
            [6.3, 2.6, 0.4],
            MAP_WIDTH / 2.0,
            height - 24.0,
        ),
        (
            Place::Willow,
            [10.6, 7.1, 0.45],
            5.0 * MAP_WIDTH / 6.0,
            height - 14.0,
        ),
    ] {
        let (from_x, fy) = project(from);
        let cx = r1(card);
        let fx = if (from_x - cx).abs() < 30.0 {
            cx
        } else {
            from_x
        };
        groups.push(format!(
            r#"<g class="dock" data-m="{}" data-want="{}"><polyline points="{fx},{fy} {fx},{lane} {cx},{lane} {cx},{height}"/><circle cx="{fx}" cy="{fy}" r="2.6"/></g>"#,
            place.name(),
            want(place).level()
        ));
    }
    PreEscaped(format!(
        "<svg class=\"map-svg\" viewBox=\"0 0 {MAP_WIDTH} {height}\" role=\"img\" aria-labelledby=\"map-title\" focusable=\"false\">\n<title id=\"map-title\">{}</title>\n{}\n</svg>",
        escaped(title),
        groups.join("\n")
    ))
}

/// One place alone, on its own patch of floor, as a card on a phone shows it.
#[must_use]
pub fn place(place: Place, want: Want, class: &str) -> Markup {
    let cuboids = boxes(place);
    let b = bounds(&cuboids);
    let pad = 6.0;
    let width = (b.max_x - b.min_x + pad * 2.0).ceil();
    let height = (b.max_y - b.min_y + pad * 2.0).ceil();
    let project = |point: Point| {
        let (a, c) = iso(point);
        (r1(a - b.min_x + pad), r1(c - b.min_y + pad))
    };
    let mut out = format!(
        r#"<svg class="{class}" viewBox="0 0 {width} {height}" aria-hidden="true" focusable="false"><g class="m" data-m="{}" data-want="{}">"#,
        place.name(),
        want.level()
    );
    draw_boxes(&mut out, &cuboids, &project);
    out.push_str("</g></svg>");
    PreEscaped(out)
}

// ── the week ────────────────────────────────────────────────────────────

/// One day of the week's drawing: its short name and the versions in each hour, `None` for an
/// hour still to come.
#[derive(Debug, Clone)]
pub struct Day {
    pub name: String,
    pub hours: [Option<u64>; 24],
}

/// The week's drawing and what it found.
#[derive(Debug)]
pub struct Week {
    pub svg: Markup,
    pub total: u64,
    /// The busiest hour: the day's index and the hour, the first one on a tie.
    pub busiest: Option<(usize, usize)>,
}

/// Versions per hour over seven days, oldest day first and today last. The hour `now` of the
/// last day is marked.
#[must_use]
pub fn week(days: &[Day], now: usize) -> Week {
    let (cell, pitch, lx, top) = (8.0, 10.0, 30.0, 4.0);
    let mut defs = String::new();
    for (k, level) in [0, 4, 8, 12, 16].into_iter().enumerate() {
        pattern(&mut defs, &format!("hd-{k}"), level, 2.0, "hd", Some("hg"));
    }
    let mut rows = String::new();
    let mut total = 0;
    let mut busiest: Option<(usize, usize, u64)> = None;
    let last = days.len().saturating_sub(1);
    for (j, day) in days.iter().enumerate() {
        let y = top + unit(j) * pitch;
        let _ = write!(
            rows,
            r#"<text class="wk-day" x="0" y="{}">{}</text>"#,
            y + cell - 0.5,
            escaped(&day.name)
        );
        for (h, count) in day.hours.iter().enumerate() {
            let Some(count) = *count else { continue };
            total += count;
            if count > busiest.map_or(0, |(_, _, most)| most) {
                busiest = Some((j, h, count));
            }
            let level = count.min(4);
            let now = if j == last && h == now { " now" } else { "" };
            let x = lx + unit(h) * pitch;
            let _ = write!(
                rows,
                r#"<rect class="wk-c{now}" x="{x}" y="{y}" width="{cell}" height="{cell}" fill="url(#hd-{level})"><title>{} {h:02}:00, {count} {}</title></rect>"#,
                escaped(&day.name),
                if count == 1 { "version" } else { "versions" }
            );
        }
    }
    for h in [0_u8, 6, 12, 18] {
        let _ = write!(
            rows,
            r#"<text class="wk-h" x="{}" y="{}">{h:02}</text>"#,
            lx + f64::from(h) * pitch,
            top + 7.0 * pitch + 8.0
        );
    }
    let (width, height) = (lx + 24.0 * pitch, top + 7.0 * pitch + 12.0);
    let most = busiest.map_or_else(String::new, |(j, h, _)| {
        format!(", the most on {} at {h:02}:00", days[j].name)
    });
    let svg = PreEscaped(format!(
        "<svg class=\"week-svg\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-labelledby=\"wk-title\">\n<title id=\"wk-title\">Versions saved per hour over the last seven days. {total} in all{}.</title>\n<defs>{defs}</defs>{rows}</svg>",
        escaped(&most)
    ));
    Week {
        svg,
        total,
        busiest: busiest.map(|(j, h, _)| (j, h)),
    }
}

// ── a page's 90 days ────────────────────────────────────────────────────

/// What changed on one day of a page's last 90.
#[derive(Debug, Clone, Copy, Default)]
pub struct Activity {
    pub versions: u64,
    /// Words added and removed.
    pub words: u64,
}

/// Words changed per day over the last 90 days, oldest first and today last. `days[0]` is
/// today, `days[89]` 89 days ago.
#[must_use]
pub fn history(days: &[Activity; 90], id: &str) -> Markup {
    let (cols, rows, cell, gap) = (15, 6, 14.0, 3.0);
    let pitch = cell + gap;
    let mut defs = String::new();
    for k in 0..=8 {
        pattern(&mut defs, &format!("hh-{k}"), k * 2, 2.0, "hd", Some("hg"));
    }
    let mut cells = String::new();
    for i in 0..90_usize {
        let ago = 89 - i;
        let day = days[ago];
        let level = if day.versions == 0 {
            0
        } else {
            8.min(3 + (day.words + 25) / 50)
        };
        let window = match ago {
            0..7 => " w7",
            7..30 => " w30",
            _ => "",
        };
        let label = match ago {
            0 => "Today".to_owned(),
            1 => "Yesterday".to_owned(),
            _ => format!("{ago} days ago"),
        };
        let what = if day.versions == 0 {
            "no change".to_owned()
        } else {
            format!(
                "{} {}, {} words changed",
                day.versions,
                if day.versions > 1 {
                    "versions"
                } else {
                    "version"
                },
                day.words
            )
        };
        let _ = write!(
            cells,
            r#"<rect class="hc{window}" x="{}" y="{}" width="{cell}" height="{cell}" fill="url(#hh-{level})"><title>{label}: {what}</title></rect>"#,
            unit(i % cols) * pitch,
            unit(i / cols) * pitch
        );
    }
    let width = unit(cols) * pitch - gap;
    let height = unit(rows) * pitch - gap;
    PreEscaped(format!(
        "<svg class=\"hist-svg\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-labelledby=\"{id}\">\n<title id=\"{id}\">Words changed per day over the last 90 days, oldest first, today last.</title>\n<defs>{defs}</defs>{cells}</svg>"
    ))
}

// ── marks ───────────────────────────────────────────────────────────────

/// The wordmark's 4x4 Bayer glyph. `extra` is more attributes, written as they are.
#[must_use]
pub fn glyph(class: &str, extra: &str) -> Markup {
    let mut out = format!(
        r#"<svg class="{class}" viewBox="0 0 4 4" aria-hidden="true" focusable="false"{extra}>"#
    );
    for (i, bayer) in BAYER.iter().enumerate() {
        let _ = write!(
            out,
            r#"<rect class="b{bayer}" x="{}" y="{}" width="1" height="1"/>"#,
            i % 4,
            i / 4
        );
    }
    out.push_str("</svg>");
    PreEscaped(out)
}

/// What the resolution mark stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Want(Want),
    /// New since the last visit.
    New,
}

/// The resolution mark: the glyph filled to how much something wants you, or marked new.
#[must_use]
pub fn resolution(mark: Mark) -> Markup {
    let level = match mark {
        Mark::Want(want) => want.level(),
        Mark::New => "new",
    };
    glyph("rm", &format!(r#" data-want="{level}""#))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_half_up_to_a_tenth_without_negative_zero() {
        assert_eq!(r1(-0.04).to_string(), "0");
        assert_eq!(r1(12.25).to_string(), "12.3");
        assert_eq!(r1(-2.25).to_string(), "-2.2");
        assert_eq!(r1(380.0).to_string(), "380");
    }

    #[test]
    fn the_map_keeps_the_prototypes_proportions() {
        assert_eq!(map_size(), (760.0, 415.0));
    }

    #[test]
    fn the_week_finds_its_busiest_hour_and_skips_hours_to_come() {
        let mut quiet = Day {
            name: "Tue".to_owned(),
            hours: [Some(0); 24],
        };
        quiet.hours[22] = Some(3);
        let mut today = Day {
            name: "Wed".to_owned(),
            hours: [None; 24],
        };
        today.hours[..10].fill(Some(1));
        let week = week(&[quiet, today], 9);
        assert_eq!((week.total, week.busiest), (13, Some((0, 22))));
        let svg = week.svg.into_string();
        assert_eq!(svg.matches("<rect class=\"wk-c").count(), 34);
        assert!(
            svg.contains(r#"<rect class="wk-c now" x="120" y="14""#),
            "{svg}"
        );
        assert!(svg.contains("13 in all, the most on Tue at 22:00."));
    }
}
