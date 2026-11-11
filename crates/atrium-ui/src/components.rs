//! The pieces several pages share, each as the prototype's `components.html` shows it.

use atrium_core::Local;
use maud::{Markup, html};

pub use crate::draw::{Mark, Want, resolution as mark};
use crate::words::number;

/// Words added and removed: `+142 −38`, read out as "added 142 removed 38".
#[must_use]
pub fn delta(added: u64, removed: u64) -> Markup {
    html! {
        span.delta {
            ins { span.vh { "added " } "+" (number(added)) }
            " "
            del { span.vh { "removed " } "−" (number(removed)) }
        }
    }
}

/// The striped mark of two machines that edited the same lines, and what it says.
#[must_use]
pub fn overlap(text: &Markup) -> Markup {
    html! { span.ov { span.ov-mark aria-hidden="true" {} (text) } }
}

/// `five new since you were last here at 07:48`.
#[must_use]
pub fn since(counted: &str, local: &Local, last: i64) -> Markup {
    html! {
        span.since {
            (mark(Mark::New))
            span { b { (counted) } " since you were last here at " (local.time(last, &local.hm(last))) }
        }
    }
}

/// The command line: a search form the palette module turns into a list as you type.
#[must_use]
pub fn cmd_line(id: &str, action: &str, pages: Option<&str>) -> Markup {
    let input = format!("{id}-q");
    let list = format!("{id}-list");
    html! {
        div.cmd data-module="palette" data-inline data-pages=[pages] {
            form.cmd-form action=(action) method="get" role="search" aria-label="Jump or search" {
                label.vh for=(input) { "Jump to a page, or search the wiki" }
                span.cmd-prompt aria-hidden="true" { "→" }
                input #(input) name="q" type="search" autocomplete="off" spellcheck="false"
                    placeholder="Jump to a page, or search the wiki" role="combobox"
                    aria-expanded="false" aria-controls=(list) aria-autocomplete="list";
                kbd.cmd-key aria-hidden="true" { "/" }
                button.cmd-go type="submit" { "Search" }
            }
            ul.cmd-list.pal-list #(list) role="listbox" aria-label="Pages" tabindex="-1" hidden {}
        }
    }
}

/// Links to the newer and older parts of a list, a button that does nothing where there is
/// no such part.
#[must_use]
pub fn pager(label: &str, text: &str, newer: Option<&str>, older: Option<&str>) -> Markup {
    html! {
        nav.pager aria-label=(label) {
            span { (text) }
            span.btns {
                @match newer {
                    Some(href) => a.btn href=(href) rel="prev" { "Newer" },
                    None => span.btn aria-disabled="true" { "Newer" },
                }
                @match older {
                    Some(href) => a.btn href=(href) rel="next" { "Older" },
                    None => span.btn aria-disabled="true" { "Older" },
                }
            }
        }
    }
}

/// A tile's heading row, with a count or note at its end.
#[must_use]
pub fn tile_head(id: &str, title: &str, more: Option<Markup>) -> Markup {
    html! {
        header.tile-head {
            h2 #(id) { (title) }
            @if let Some(more) = more { (more) }
        }
    }
}

/// The quiet tile that stands where a tile could not be filled in time.
#[must_use]
pub fn away_tile(id: &str, title: &str) -> Markup {
    let heading = format!("{id}-h");
    html! {
        article.tile.away-note.surface data-surface aria-labelledby=(heading) {
            (tile_head(&heading, title, None))
            p { "The wiki is briefly away. This part comes back when you load the page again." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delta_reads_out_its_signs() {
        assert_eq!(
            delta(1420, 38).into_string(),
            r#"<span class="delta"><ins><span class="vh">added </span>+1,420</ins> <del><span class="vh">removed </span>−38</del></span>"#
        );
    }
}
