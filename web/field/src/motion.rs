//! What the field asks of each surface, and the springs that ease it there. Plain
//! functions, tested natively.

/// How much the field moves: the reader's setting, or still under reduced motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Still,
    Calm,
    Normal,
    Lively,
}

impl Mode {
    /// The mode for an intensity setting, unless the reader asked for reduced motion.
    #[must_use]
    pub fn new(reduced: bool, intensity: Option<&str>) -> Self {
        match (reduced, intensity) {
            (true, _) => Self::Still,
            (false, Some("calm")) => Self::Calm,
            (false, Some("lively")) => Self::Lively,
            _ => Self::Normal,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Still => "still",
            Self::Calm => "calm",
            Self::Normal => "normal",
            Self::Lively => "lively",
        }
    }

    /// Calm and still hold the field: no drift, no breath, no pointer.
    #[must_use]
    pub fn quiet(self) -> bool {
        matches!(self, Self::Still | Self::Calm)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Spring {
    pub x: f64,
    v: f64,
}

impl Spring {
    /// Moves toward `target` over `dt` seconds, and says whether it is still moving. Still
    /// jumps there at once.
    pub fn step(&mut self, target: f64, dt: f64, mode: Mode) -> bool {
        if mode == Mode::Still {
            *self = Self { x: target, v: 0.0 };
            return false;
        }
        let stiffness = if mode == Mode::Calm { 120.0 } else { 170.0 };
        let damping = match mode {
            Mode::Lively => 15.0,
            Mode::Calm => 24.0,
            _ => 19.0,
        };
        let acceleration = stiffness * (target - self.x) - damping * self.v;
        self.v += acceleration * dt;
        self.x += self.v * dt;
        if (target - self.x).abs() > 0.002 || self.v.abs() > 0.002 {
            return true;
        }
        *self = Self { x: target, v: 0.0 };
        false
    }
}

/// What a surface asks of the field, as gain, reach in pixels and heat. One that needs you
/// (`data-want="2"`) pulls the finest lattice far out in the colour of trouble; hover or
/// focus resolves any surface.
#[must_use]
pub fn ask(want: Option<&str>, active: bool, quiet: bool, reading: bool) -> [f32; 3] {
    match want {
        Some("2") => [1.0, 96.0, if active { 0.5 } else { 0.8 }],
        _ if active => [if quiet { 0.8 } else { 0.95 }, 56.0, 0.0],
        Some("1") => [0.72, 70.0, 0.0],
        _ if reading => [0.3, 48.0, 0.0],
        _ => [0.16, 34.0, 0.0],
    }
}

/// The pull of a machine on the map: a negative gain is heat, the colour of trouble.
#[must_use]
pub fn machine(want: u8, active: bool) -> (f64, f64) {
    let target = match want {
        2 => -0.85,
        _ if active => 0.95,
        1 => 0.5,
        _ => 0.0,
    };
    (target, if want == 2 { 95.0 } else { 70.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_holds_the_field_still() {
        assert_eq!(Mode::new(true, Some("lively")), Mode::Still);
        assert_eq!(Mode::new(false, Some("lively")), Mode::Lively);
        assert_eq!(Mode::new(false, None), Mode::Normal);
    }

    #[test]
    fn a_spring_settles_on_its_target() {
        let mut spring = Spring::default();
        let mut steps = 0;
        while spring.step(1.0, 0.016, Mode::Normal) {
            steps += 1;
            assert!(steps < 200, "settles");
        }
        assert!((spring.x - 1.0).abs() < f64::EPSILON);
        assert!(steps > 5, "eases over several frames, {steps}");
    }

    #[test]
    fn still_jumps_to_the_target() {
        let mut spring = Spring::default();
        assert!(!spring.step(0.7, 0.016, Mode::Still));
        assert!((spring.x - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn what_wants_you_asks_most() {
        assert_eq!(ask(Some("2"), false, false, false), [1.0, 96.0, 0.8]);
        assert_eq!(ask(Some("1"), true, false, false), [0.95, 56.0, 0.0]);
        assert_eq!(ask(Some("1"), false, false, false), [0.72, 70.0, 0.0]);
        assert_eq!(ask(None, false, false, true), [0.3, 48.0, 0.0]);
        assert_eq!(ask(None, false, false, false), [0.16, 34.0, 0.0]);
        assert_eq!(machine(2, true), (-0.85, 95.0));
        assert_eq!(machine(0, true), (0.95, 70.0));
    }
}
