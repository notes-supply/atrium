//! Who is reading, when, and how they like the page: the visit every request carries. Every
//! time a page shows is in the reader's time zone and reads relative to now.

use jiff::civil::Weekday;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use maud::{Markup, html};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const fn weekday(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

/// The reader's now, and every other time as the reader would say it.
#[derive(Debug, Clone)]
pub struct Local {
    now: Zoned,
}

impl Local {
    /// The reader's clock at `now`, in `zone`.
    #[must_use]
    pub fn new(now: Timestamp, zone: TimeZone) -> Self {
        Self {
            now: now.to_zoned(zone),
        }
    }

    /// Now, in the reader's zone.
    #[must_use]
    pub const fn now(&self) -> &Zoned {
        &self.now
    }

    /// Now in seconds since the Unix epoch.
    #[must_use]
    pub fn seconds(&self) -> i64 {
        self.now.timestamp().as_second()
    }

    /// The hour of the day, 0 to 23.
    #[must_use]
    pub fn hour(&self) -> u8 {
        u8::try_from(self.now.hour()).unwrap_or(0)
    }

    /// A time in seconds since the Unix epoch, in the reader's zone.
    #[must_use]
    pub fn at(&self, seconds: i64) -> Zoned {
        Timestamp::from_second(seconds)
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_zoned(self.now.time_zone().clone())
    }

    /// `09:12`.
    #[must_use]
    pub fn hm(&self, seconds: i64) -> String {
        let at = self.at(seconds);
        format!("{:02}:{:02}", at.hour(), at.minute())
    }

    /// Calendar days between that day and today: 0 today, 1 yesterday.
    #[must_use]
    pub fn days_ago(&self, seconds: i64) -> i64 {
        i64::from(
            self.at(seconds)
                .date()
                .until(self.now.date())
                .map_or(0, |span| span.get_days()),
        )
    }

    /// `9 Nov`, with the year when it is not this one.
    #[must_use]
    pub fn date_short(&self, seconds: i64) -> String {
        let at = self.at(seconds);
        let month = &MONTHS[usize::try_from(at.month() - 1).unwrap_or(0)][..3];
        if at.year() == self.now.year() {
            format!("{} {month}", at.day())
        } else {
            format!("{} {month} {}", at.day(), at.year())
        }
    }

    /// `Wednesday 11 November`.
    #[must_use]
    pub fn date_long(&self, seconds: i64) -> String {
        let at = self.at(seconds);
        format!(
            "{} {} {}",
            weekday(at.weekday()),
            at.day(),
            MONTHS[usize::try_from(at.month() - 1).unwrap_or(0)]
        )
    }

    /// `Today`, `Yesterday` or `Monday 9 November`.
    #[must_use]
    pub fn day_name(&self, seconds: i64) -> String {
        match self.days_ago(seconds) {
            0 => "Today".to_owned(),
            1 => "Yesterday".to_owned(),
            _ => self.date_long(seconds),
        }
    }

    /// The short weekday, `Mon`.
    #[must_use]
    pub fn weekday_short(&self, seconds: i64) -> &'static str {
        &weekday(self.at(seconds).weekday())[..3]
    }

    /// The full weekday, `Monday`.
    #[must_use]
    pub fn weekday(&self, seconds: i64) -> &'static str {
        weekday(self.at(seconds).weekday())
    }

    /// `09:12` today, `Mon 21:14` this week, `9 Nov` before.
    #[must_use]
    pub fn when(&self, seconds: i64) -> String {
        match self.days_ago(seconds) {
            0 => self.hm(seconds),
            1..6 => format!("{} {}", self.weekday_short(seconds), self.hm(seconds)),
            _ => self.date_short(seconds),
        }
    }

    /// `today at 09:12`, `yesterday at 21:14` or `9 Nov at 11:02`.
    #[must_use]
    pub fn when_long(&self, seconds: i64) -> String {
        match self.days_ago(seconds) {
            0 => format!("today at {}", self.hm(seconds)),
            1 => format!("yesterday at {}", self.hm(seconds)),
            _ => format!("{} at {}", self.date_short(seconds), self.hm(seconds)),
        }
    }

    /// `29 minutes ago`, rounded as a person would.
    #[must_use]
    pub fn ago(&self, seconds: i64) -> String {
        let minutes = (self.seconds() - seconds + 30).div_euclid(60);
        match minutes {
            ..2 => "a minute ago".to_owned(),
            2..60 => format!("{minutes} minutes ago"),
            60..120 => "an hour ago".to_owned(),
            120..1440 => format!("{} hours ago", (minutes + 30) / 60),
            _ => format!("{} days ago", (minutes + 720) / 1440),
        }
    }

    /// The machine-readable form of a time, for `datetime`.
    #[must_use]
    pub fn iso(&self, seconds: i64) -> String {
        self.at(seconds)
            .strftime("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    }

    /// A `<time>` element for `seconds` that reads `text`.
    #[must_use]
    pub fn time(&self, seconds: i64, text: &str) -> Markup {
        html! { time datetime=(self.iso(seconds)) { (text) } }
    }
}

/// The day or night look, when the reader chose one. Without it the system's applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// How much the background field moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Intensity {
    Calm,
    #[default]
    Normal,
    Lively,
}

impl Intensity {
    pub const ALL: [Self; 3] = [Self::Calm, Self::Normal, Self::Lively];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Calm => "calm",
            Self::Normal => "normal",
            Self::Lively => "lively",
        }
    }
}

/// The reader's settings, kept in cookies so the server can draw the page with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    pub theme: Option<Theme>,
    pub intensity: Intensity,
    /// Whether keys without a modifier, such as `j` or `1`, do anything.
    pub letter_keys: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: None,
            intensity: Intensity::Normal,
            letter_keys: true,
        }
    }
}

impl Prefs {
    /// The cookie that holds the theme.
    pub const THEME: &str = "atrium-theme";
    /// The cookie that holds the intensity.
    pub const INTENSITY: &str = "atrium-intensity";
    /// The cookie that holds whether letter keys work: `on` or `off`.
    pub const KEYS: &str = "atrium-keys";
    /// How long each cookie is kept, in seconds: a year.
    pub const KEEP: u32 = 365 * 24 * 60 * 60;

    /// The settings named by a request's cookies. Values it does not know are ignored.
    #[must_use]
    pub fn from_cookies<'a>(cookies: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut prefs = Self::default();
        for (name, value) in cookies {
            if let Some(choice) = Choice::ALL
                .into_iter()
                .find(|choice| choice.cookie() == name && choice.kept() == Some(value))
            {
                prefs.choose(choice);
            }
        }
        prefs
    }

    /// Takes `choice` in place of what the reader had for its setting.
    pub const fn choose(&mut self, choice: Choice) {
        match choice {
            Choice::Theme(theme) => self.theme = theme,
            Choice::Intensity(level) => self.intensity = level,
            Choice::LetterKeys(on) => self.letter_keys = on,
        }
    }
}

/// One choice the settings page offers: a setting and its value, as its form sends them, and
/// the cookie that keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// The day or night look, or `None` to follow the system's.
    Theme(Option<Theme>),
    Intensity(Intensity),
    /// Whether single-letter keys work.
    LetterKeys(bool),
}

impl Choice {
    /// Every choice, in the order the settings page lists them.
    pub const ALL: [Self; 8] = [
        Self::Theme(None),
        Self::Theme(Some(Theme::Light)),
        Self::Theme(Some(Theme::Dark)),
        Self::Intensity(Intensity::Calm),
        Self::Intensity(Intensity::Normal),
        Self::Intensity(Intensity::Lively),
        Self::LetterKeys(true),
        Self::LetterKeys(false),
    ];

    /// The setting's name in the form: `theme`, `intensity` or `keys`, which is its cookie's
    /// name after `atrium-` and its attribute's on `<html>` after `data-`.
    #[must_use]
    pub const fn setting(self) -> &'static str {
        match self {
            Self::Theme(_) => "theme",
            Self::Intensity(_) => "intensity",
            Self::LetterKeys(_) => "keys",
        }
    }

    /// The value in the form: `system`, `light` or `dark`, an intensity's name, `on` or `off`.
    #[must_use]
    pub const fn value(self) -> &'static str {
        match self {
            Self::Theme(None) => "system",
            Self::Theme(Some(theme)) => theme.name(),
            Self::Intensity(level) => level.name(),
            Self::LetterKeys(true) => "on",
            Self::LetterKeys(false) => "off",
        }
    }

    /// The choice a form sends as `setting=value`, when both are known.
    #[must_use]
    pub fn parse(setting: &str, value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.setting() == setting && choice.value() == value)
    }

    /// The cookie that keeps the setting.
    #[must_use]
    pub const fn cookie(self) -> &'static str {
        match self {
            Self::Theme(_) => Prefs::THEME,
            Self::Intensity(_) => Prefs::INTENSITY,
            Self::LetterKeys(_) => Prefs::KEYS,
        }
    }

    /// What the cookie keeps, or `None` when the choice clears it, as following the system
    /// does.
    #[must_use]
    pub const fn kept(self) -> Option<&'static str> {
        match self {
            Self::Theme(None) => None,
            _ => Some(self.value()),
        }
    }

    /// Whether the reader has this choice.
    #[must_use]
    pub fn chosen(self, prefs: Prefs) -> bool {
        let mut chosen = prefs;
        chosen.choose(self);
        chosen == prefs
    }
}

/// How a page comes in: the whole entrance for a reader arriving, a short one for the next
/// page of a sitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Entrance {
    #[default]
    Full,
    Short,
}

impl Entrance {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Short => "short",
        }
    }
}

/// One request's reader: their clock, when they were last here, and their settings.
#[derive(Debug, Clone)]
pub struct Visit {
    pub local: Local,
    /// When the reader's previous visit ended, if Atrium knows of one.
    pub last: Option<i64>,
    pub prefs: Prefs,
    pub entrance: Entrance,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> Local {
        // Wednesday 11 November 2026, 09:41 in Stockholm.
        Local::new(
            "2026-11-11T08:41:00Z".parse().unwrap(),
            TimeZone::get("Europe/Stockholm").unwrap(),
        )
    }

    fn at(text: &str) -> i64 {
        text.parse::<Timestamp>().unwrap().as_second()
    }

    #[test]
    fn reads_times_in_the_readers_zone() {
        let local = local();
        assert_eq!(local.hour(), 9);
        assert_eq!(local.hm(at("2026-11-11T08:12:00Z")), "09:12");
        assert_eq!(local.date_long(local.seconds()), "Wednesday 11 November");
        assert_eq!(
            local.iso(at("2026-11-11T08:12:00Z")),
            "2026-11-11T09:12:00+01:00"
        );
    }

    #[test]
    fn says_when_relative_to_today() {
        let local = local();
        assert_eq!(local.when(at("2026-11-11T08:12:00Z")), "09:12");
        assert_eq!(local.when(at("2026-11-10T21:48:00Z")), "Tue 22:48");
        assert_eq!(local.when(at("2026-11-02T10:00:00Z")), "2 Nov");
        assert_eq!(local.when(at("2025-11-02T10:00:00Z")), "2 Nov 2025");
        assert_eq!(
            local.when_long(at("2026-11-10T23:30:00Z")),
            "today at 00:30"
        );
        assert_eq!(
            local.when_long(at("2026-11-10T22:30:00Z")),
            "yesterday at 23:30"
        );
        assert_eq!(
            local.day_name(at("2026-11-09T12:00:00Z")),
            "Monday 9 November"
        );
    }

    #[test]
    fn says_how_long_ago() {
        let local = local();
        let now = local.seconds();
        assert_eq!(local.ago(now - 50), "a minute ago");
        assert_eq!(local.ago(now - 29 * 60), "29 minutes ago");
        assert_eq!(local.ago(now - 90 * 60), "an hour ago");
        assert_eq!(local.ago(now - 150 * 60), "3 hours ago");
        assert_eq!(local.ago(now - 60 * 60 * 60), "3 days ago");
    }

    #[test]
    fn prefs_come_from_cookies_and_ignore_what_they_do_not_know() {
        let prefs = Prefs::from_cookies([
            ("atrium-theme", "dark"),
            ("atrium-intensity", "lively"),
            ("atrium-keys", "off"),
            ("other", "x"),
        ]);
        assert_eq!(prefs.theme, Some(Theme::Dark));
        assert_eq!(prefs.intensity, Intensity::Lively);
        assert!(!prefs.letter_keys);
        let odd = Prefs::from_cookies([
            ("atrium-theme", "sepia"),
            ("atrium-theme", "system"),
            ("atrium-intensity", "loud"),
        ]);
        assert_eq!(odd, Prefs::default());
    }

    #[test]
    fn a_choice_reads_back_from_its_form_and_its_cookie() {
        for choice in Choice::ALL {
            assert_eq!(
                Choice::parse(choice.setting(), choice.value()),
                Some(choice)
            );
            assert_eq!(
                choice.cookie(),
                ["atrium-", choice.setting()].concat(),
                "{choice:?}"
            );
            let mut prefs = Prefs::default();
            prefs.choose(choice);
            let kept = choice.kept().map(|value| (choice.cookie(), value));
            assert_eq!(Prefs::from_cookies(kept), prefs, "{choice:?}");
            assert!(choice.chosen(prefs), "{choice:?}");
        }
        for (setting, value) in [
            ("theme", "sepia"),
            ("theme", "Dark"),
            ("intensity", "system"),
            ("keys", "true"),
            ("atrium-theme", "dark"),
            ("", ""),
        ] {
            assert_eq!(Choice::parse(setting, value), None, "{setting}={value}");
        }
    }

    #[test]
    fn following_the_system_is_chosen_until_a_theme_is() {
        let system = Choice::Theme(None);
        assert!(system.chosen(Prefs::default()));
        assert_eq!(system.kept(), None);
        let night = Prefs::from_cookies([("atrium-theme", "dark")]);
        assert!(!system.chosen(night));
        assert!(Choice::Theme(Some(Theme::Dark)).chosen(night));
        assert!(Choice::Intensity(Intensity::Normal).chosen(night));
        assert!(!Choice::Intensity(Intensity::Calm).chosen(night));
    }
}
