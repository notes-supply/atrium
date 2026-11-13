//! The settings page: the theme, the field and the single-letter keys, each a group of buttons
//! that post the reader's choice, and the keys every page answers.

use atrium_core::{Choice, Intensity, Prefs, Theme};
use maud::{Markup, html};

use crate::draw::glyph;
use crate::{Current, MAIN, Page, Shell};

/// The settings page's address, which its forms post to.
pub const PATH: &str = "/settings";

/// A choice, the name its button shows, and what it does.
type Offer = (Choice, &'static str, &'static str);

const THEME: [Offer; 3] = [
    (
        Choice::Theme(None),
        "Follow the system",
        "Day or night, as this device is set.",
    ),
    (
        Choice::Theme(Some(Theme::Light)),
        "Light",
        "Day, whatever this device is set to.",
    ),
    (
        Choice::Theme(Some(Theme::Dark)),
        "Night",
        "Night, whatever this device is set to.",
    ),
];

const FIELD: [Offer; 3] = [
    (
        Choice::Intensity(Intensity::Calm),
        "Calm",
        "The field holds still.",
    ),
    (
        Choice::Intensity(Intensity::Normal),
        "Normal",
        "The field breathes and follows the pointer.",
    ),
    (
        Choice::Intensity(Intensity::Lively),
        "Lively",
        "A trail, ripples on click, packets on the lines.",
    ),
];

const KEYS: [Offer; 2] = [
    (
        Choice::LetterKeys(true),
        "On",
        "Keys such as j and 1 work without Ctrl.",
    ),
    (
        Choice::LetterKeys(false),
        "Off",
        "Only keys with Ctrl or Cmd work.",
    ),
];

/// The page, with the reader's choice in each setting pressed.
#[must_use]
pub fn page(shell: &Shell) -> Page {
    let prefs = shell.visit().prefs;
    let sections = shell.sections();
    let body = html! {
        main #(MAIN) .settings {
            header.page-head {
                h1 { "Settings" }
                p.sub { "How Atrium looks and which keys it answers, in this browser. Each choice is kept in a cookie for a year." }
            }
            div.settings-list.surface data-surface {
                (setting("Theme", &THEME, prefs))
                (setting("Field", &FIELD, prefs))
                (setting("Single-letter keys", &KEYS, prefs))
            }
            section.keyboard.surface data-surface aria-labelledby="keyboard-h" {
                h2 #keyboard-h .rail-h { "Keyboard" }
                dl.key-list {
                    @if sections > 0 {
                        div { dt { kbd { "1" } "–" kbd { (sections) } } dd { "sections" } }
                    }
                    div { dt { kbd { "j" } " " kbd { "k" } } dd { "rows" } }
                    div { dt { kbd { "/" } } dd { "search" } }
                    div { dt { kbd.kbd-ctrl { "Ctrl K" } } dd { "jump" } }
                    div { dt { kbd { "T" } } dd { "theme" } }
                    div { dt { kbd { "F" } } dd { "field" } }
                }
            }
        }
    };
    let mut page = Page::new(
        "Settings, Atrium",
        "The theme, the field and the single-letter keys, and the keys every page answers.",
        body,
    );
    page.current = Current::Nothing;
    page
}

/// One setting as a form whose buttons each post one choice. The browser module takes the
/// same buttons in place, without a reload.
fn setting(title: &str, offers: &[Offer], prefs: Prefs) -> Markup {
    let name = offers.first().map_or("", |(choice, ..)| choice.setting());
    let heading = format!("{name}-h");
    html! {
        form.setting.prefs method="post" action=(PATH) {
            h2 #(heading) { (title) }
            div.choices role="group" aria-labelledby=(heading) {
                @for (choice, label, note) in offers {
                    @let note_id = format!("{name}-{}", choice.value());
                    div.choice {
                        button.btn type="submit" name=(name) value=(choice.value())
                            aria-pressed=(choice.chosen(prefs)) aria-describedby=(note_id)
                        {
                            @if let Choice::Intensity(level) = choice {
                                (glyph(&format!("sw sw-{}", level.name()), ""))
                            }
                            span { (label) }
                        }
                        p.choice-note #(note_id) { (note) }
                    }
                }
            }
        }
    }
}
