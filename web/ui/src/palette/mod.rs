//! The palette: jump anywhere by name. Ctrl or Cmd K opens it as a dialog from any page; on
//! home the command line is the same list, inline. Enter on a query with no page by that
//! name searches the wiki. Every destination is also a plain link on the page.
//!
//! The matching and the marking are plain functions, tested natively. The browser module
//! wires them to the page.

mod browser;

pub use browser::{mount, toggle};

/// Where a query matched a label, and how well.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub score: f64,
    /// The index of each matched character in the label.
    pub at: Vec<usize>,
}

/// Whether every character of `query`, spaces aside, appears in `label` in order. Runs of
/// consecutive characters and characters that start a word score higher, and a shorter
/// label wins a tie. Both are already lowercase.
#[must_use]
pub fn find(query: &str, label: &str) -> Option<Hit> {
    if query.is_empty() {
        return Some(Hit {
            score: 1.0,
            at: Vec::new(),
        });
    }
    let label: Vec<char> = label.chars().collect();
    let (mut from, mut run, mut score) = (0, 0.0, 0.0);
    let mut at = Vec::new();
    for wanted in query.chars().filter(|c| *c != ' ') {
        let found = from + label.get(from..)?.iter().position(|c| *c == wanted)?;
        run = if found == from { run + 1.0 } else { 0.0 };
        let word_start = found == 0 || label[found - 1] == ' ';
        score += 1.0 + run * 2.0 + if word_start { 3.0 } else { 0.0 };
        at.push(found);
        from = found + 1;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "labels are far shorter than 2^52"
    )]
    let length = label.len() as f64;
    Some(Hit {
        score: score - length * 0.01,
        at,
    })
}

/// Appends `text` to `html`, escaped for an element or a quoted attribute.
pub fn escape(text: &str, html: &mut String) {
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            _ => html.push(c),
        }
    }
}

/// Appends `label` to `html` with the characters at `at` in bold, one `<b>` per run.
pub fn mark(label: &str, at: &[usize], html: &mut String) {
    let mut bold = false;
    let mut buffer = [0; 4];
    for (index, c) in label.chars().enumerate() {
        let hit = at.contains(&index);
        if hit != bold {
            html.push_str(if hit { "<b>" } else { "</b>" });
            bold = hit;
        }
        escape(c.encode_utf8(&mut buffer), html);
    }
    if bold {
        html.push_str("</b>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(
            find("", "anything"),
            Some(Hit {
                score: 1.0,
                at: vec![]
            })
        );
    }

    #[test]
    fn the_characters_must_appear_in_order() {
        assert_eq!(
            find("cor", "keyboard build: corne v4").unwrap().at,
            [16, 17, 18]
        );
        assert_eq!(find("c v", "corne v4").unwrap().at, [0, 6]);
        assert!(find("rc", "corne").is_none());
        assert!(find("x", "corne").is_none());
    }

    #[test]
    fn word_starts_and_runs_rank_first() {
        let start = find("cor", "corne").unwrap().score;
        let inside = find("cor", "decorne").unwrap().score;
        let scattered = find("cor", "cxoxr").unwrap().score;
        assert!(start > inside, "{start} > {inside}");
        assert!(inside > scattered, "{inside} > {scattered}");
        assert!(find("a", "ab").unwrap().score > find("a", "abc").unwrap().score);
    }

    #[test]
    fn marks_each_run_once_and_escapes() {
        let mut html = String::new();
        mark("a<b&c", &[0, 1, 4], &mut html);
        assert_eq!(html, "<b>a&lt;</b>b&amp;<b>c</b>");
    }
}
