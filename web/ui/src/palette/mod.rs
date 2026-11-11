//! The command palette. The page lists every command as a plain link inside the element
//! marked `data-module="palette"`; this module reads those links, opens a filtered copy of
//! them in a dialog on Ctrl+K or Cmd+K, and follows a command when its keys are typed.
//!
//! The matching and the key sequences are plain functions, tested natively. The browser
//! module wires them to the page.

mod browser;

pub use browser::mount;

/// Whether `label` matches `query`: every word of the query appears in it, without case.
#[must_use]
pub fn matches(query: &str, label: &str) -> bool {
    let label = label.to_lowercase();
    query
        .split_whitespace()
        .all(|word| label.contains(&word.to_lowercase()))
}

/// What a key typed outside a text field does.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Run the command at this index.
    Run(usize),
    /// Wait for the next key, with these keys typed so far.
    Wait(String),
    /// Nothing: the keys start no command.
    Nothing,
}

/// The step after `key`, when `typed` are the keys typed just before it and `sequences` are
/// each command's keys, such as `g w`, in command order. A key that cannot continue `typed`
/// may start a sequence of its own.
#[must_use]
pub fn step(typed: &str, key: &str, sequences: &[Option<&str>]) -> Step {
    let continued = (!typed.is_empty()).then(|| format!("{typed} {key}"));
    for candidate in continued.into_iter().chain([key.to_owned()]) {
        if let Some(index) = sequences
            .iter()
            .position(|keys| *keys == Some(candidate.as_str()))
        {
            return Step::Run(index);
        }
        let prefix = format!("{candidate} ");
        if sequences
            .iter()
            .flatten()
            .any(|keys| keys.starts_with(&prefix))
        {
            return Step::Wait(candidate);
        }
    }
    Step::Nothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_must_appear_without_case() {
        assert!(matches("", "Recent changes"));
        assert!(matches("CHAN rec", "Recent changes"));
        assert!(!matches("changes wiki", "Recent changes"));
    }

    const KEYS: [Option<&str>; 4] = [Some("g h"), None, Some("g w"), Some("/")];

    #[test]
    fn a_sequence_runs_its_command() {
        assert_eq!(step("", "g", &KEYS), Step::Wait("g".to_owned()));
        assert_eq!(step("g", "w", &KEYS), Step::Run(2));
        assert_eq!(step("", "/", &KEYS), Step::Run(3));
    }

    #[test]
    fn a_key_that_breaks_a_sequence_can_start_one() {
        assert_eq!(step("g", "x", &KEYS), Step::Nothing);
        assert_eq!(step("g", "g", &KEYS), Step::Wait("g".to_owned()));
        assert_eq!(step("", "w", &KEYS), Step::Nothing);
    }
}
