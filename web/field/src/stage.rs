//! How much of the field this device can afford, decided from the field's own frames. Plain
//! functions, tested natively.
//!
//! Each frame the field draws leaves a GPU fence, and the next frame it draws asks whether
//! that fence has passed. A frame still unfinished when the next is due is late: the GPU
//! cannot keep the field's own rate. That counts what the field asked of the GPU, and not
//! the gap between frames, so a page whose main thread is busy for other reasons, which
//! delays both frames alike, makes no frame late.
//!
//! A late frame adds the time since the last one, at most 100 ms, and a frame on time takes
//! as much away. Once half a second of late frames has gathered, the field steps down one
//! stage and starts counting again. A hitch or a hidden tab makes a frame or a few late, not
//! half a second of them; a field slow from its first frame steps down half a second in.
//! It never steps back up within a page.

/// A cheaper version of the same picture at each step, ending at the still frame that
/// reduced motion draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// One canvas pixel per CSS pixel, at the display's rate.
    Full,
    /// One canvas pixel per two CSS pixels each way: a quarter of the lattice pass, which
    /// is nearly all of the field's GPU time.
    Half,
    /// Half, and at most ten frames a second, which also cuts the compositing of a
    /// full-window layer and the reading of the page each frame.
    Slow,
    /// Half and slow, and still: no breath, no drift, no pointer, drawn only on change.
    Still,
}

/// How much late time steps the field down, and the most one frame adds, in milliseconds.
pub const PATIENCE: f64 = 500.0;
pub const FRAME: f64 = 100.0;

impl Stage {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Half => "half",
            Self::Slow => "slow",
            Self::Still => "still",
        }
    }

    /// CSS pixels per canvas pixel, each way.
    #[must_use]
    pub fn scale(self) -> u32 {
        if self == Self::Full { 1 } else { 2 }
    }

    /// The least time between two drawn frames, in milliseconds.
    #[must_use]
    pub fn interval(self) -> f64 {
        match self {
            Self::Full | Self::Half => 0.0,
            Self::Slow | Self::Still => 100.0,
        }
    }

    fn next(self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::Half),
            Self::Half => Some(Self::Slow),
            Self::Slow => Some(Self::Still),
            Self::Still => None,
        }
    }
}

/// One step down: to which stage, how far into the page in milliseconds, and how many of
/// the frames counted since the last step were late.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub to: Stage,
    pub at: f64,
    pub late: u32,
    pub of: u32,
}

#[derive(Debug, Clone)]
pub struct Governor {
    pub stage: Stage,
    /// Held at full quality, with `?perf=full`: it still counts, but never steps.
    pub held: bool,
    /// Late time gathered, in milliseconds, and when the last frame was counted.
    pub owed: f64,
    last: f64,
    /// Frames counted, and those late, since the last step.
    late: u32,
    of: u32,
    pub steps: Vec<Step>,
}

impl Governor {
    #[must_use]
    pub fn new(held: bool) -> Self {
        Self {
            stage: Stage::Full,
            held,
            owed: 0.0,
            last: 0.0,
            late: 0,
            of: 0,
            steps: Vec::new(),
        }
    }

    /// Counts one frame, late or not, `at` milliseconds into the page, and says whether the
    /// field stepped down on it.
    pub fn record(&mut self, late: bool, at: f64) -> bool {
        let span = (at - self.last).clamp(0.0, FRAME);
        self.last = at;
        self.of += 1;
        if late {
            self.late += 1;
            self.owed += span;
        } else {
            self.owed = (self.owed - span).max(0.0);
        }
        if self.held || self.owed < PATIENCE {
            return false;
        }
        let Some(next) = self.stage.next() else {
            return false;
        };
        self.steps.push(Step {
            to: next,
            at,
            late: self.late,
            of: self.of,
        });
        self.stage = next;
        self.owed = 0.0;
        self.late = 0;
        self.of = 0;
        true
    }

    /// How many of the frames counted since the last step were late, and of how many.
    #[must_use]
    pub fn late(&self) -> (u32, u32) {
        (self.late, self.of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts `frames`, one every `every` milliseconds from `from`, and says how many steps
    /// they made.
    fn run(governor: &mut Governor, from: f64, every: f64, frames: &[bool]) -> usize {
        frames
            .iter()
            .zip(1_u32..)
            .filter(|&(&late, n)| governor.record(late, from + every * f64::from(n)))
            .count()
    }

    #[test]
    fn a_fast_field_stays_full() {
        let mut governor = Governor::new(false);
        assert_eq!(run(&mut governor, 0.0, 16.7, &[false; 5000]), 0);
        assert_eq!(governor.stage, Stage::Full);
        assert_eq!(governor.late(), (0, 5000));
    }

    #[test]
    fn a_burst_at_the_start_or_a_hitch_later_does_not_step_down() {
        let mut governor = Governor::new(false);
        // The page's own start: 12 late frames in 0.4 s.
        assert_eq!(run(&mut governor, 0.0, 33.0, &[true; 12]), 0);
        assert_eq!(run(&mut governor, 400.0, 33.0, &[false; 30]), 0);
        // One frame after a hidden tab, another after a long pause, one 300 ms hitch.
        assert!(!governor.record(true, 30_000.0));
        assert!(!governor.record(true, 31_000.0));
        assert!(!governor.record(true, 31_300.0));
        assert_eq!(governor.stage, Stage::Full);
    }

    #[test]
    fn a_field_slow_from_its_first_frame_steps_down_after_half_a_second() {
        let mut governor = Governor::new(false);
        assert_eq!(run(&mut governor, 0.0, 75.0, &[true; 6]), 0);
        assert!(governor.record(true, 525.0));
        assert_eq!(governor.stage, Stage::Half);
        assert_eq!(
            governor.steps,
            [Step {
                to: Stage::Half,
                at: 525.0,
                late: 7,
                of: 7
            }]
        );
        assert_eq!(governor.late(), (0, 0), "counts again");
    }

    #[test]
    fn steps_through_every_stage_to_still_and_stays_there() {
        let mut governor = Governor::new(false);
        assert_eq!(run(&mut governor, 0.0, 100.0, &[true; 100]), 3);
        assert_eq!(governor.stage, Stage::Still);
        let stages: Vec<_> = governor.steps.iter().map(|step| step.to).collect();
        assert_eq!(stages, [Stage::Half, Stage::Slow, Stage::Still]);
        let times: Vec<_> = governor.steps.iter().map(|step| step.at).collect();
        assert_eq!(times, [500.0, 1000.0, 1500.0]);
        assert_eq!(
            run(&mut governor, 10_000.0, 16.7, &[false; 100]),
            0,
            "never up"
        );
        assert_eq!(governor.stage, Stage::Still);
    }

    #[test]
    fn mostly_late_steps_down_later_and_half_late_never() {
        let mut governor = Governor::new(false);
        let half: Vec<bool> = (0..1000).map(|n| n % 2 == 0).collect();
        assert_eq!(run(&mut governor, 0.0, 33.0, &half), 0);
        let mostly: Vec<bool> = (0..45).map(|n| n % 4 != 0).collect();
        assert_eq!(run(&mut governor, 33_000.0, 33.0, &mostly), 1);
        let step = governor.steps[0];
        assert!(step.at > 33_000.0 + 1000.0, "{step:?}");
        assert!(step.at < 33_000.0 + 1700.0, "{step:?}");
    }

    #[test]
    fn held_counts_but_never_steps() {
        let mut governor = Governor::new(true);
        assert_eq!(run(&mut governor, 0.0, 100.0, &[true; 50]), 0);
        assert_eq!(governor.stage, Stage::Full);
        assert_eq!(governor.late(), (50, 50));
    }

    #[test]
    fn each_stage_is_cheaper_than_the_last() {
        let stages = [Stage::Full, Stage::Half, Stage::Slow, Stage::Still];
        for pair in stages.windows(2) {
            assert!(pair[1].scale() >= pair[0].scale());
            assert!(pair[1].interval() >= pair[0].interval());
            assert_ne!(pair[0], pair[1]);
        }
        assert_eq!(Stage::Full.scale(), 1);
        assert!(Stage::Full.interval().abs() < f64::EPSILON);
    }
}
