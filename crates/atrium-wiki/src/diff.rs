//! `/wiki/page/{id}/diff`: the words added and removed between two versions, as paragraphs
//! and as lines. The prototype's `diffView`.

use atrium_ui::components::{delta, overlap};
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use maud::{Markup, html};

use crate::article::{all_events, number_of};
use crate::mneme::{CommitId, DiffLine, Event, Events, Mneme, Span};
use crate::routes::{Failure, Params, choice, href, losers, page_href, page_id, params, respond};

/// Versions each picker offers, newest first.
const CHOICES: usize = 50;

pub async fn show(
    shell: Shell,
    State(mneme): State<Mneme>,
    Path(id): Path<String>,
    query: Result<Query<Params>, QueryRejection>,
) -> Response {
    let shown = compare(&shell, &mneme, &id, query).await;
    respond(&shell, &mneme, shown)
}

/// One paragraph of the word view: unchanged, or changed with its words marked.
#[derive(Debug, PartialEq, Eq)]
enum Paragraph {
    Same(String),
    Changed(Vec<Piece>),
}

/// Part of a changed paragraph.
#[derive(Debug, PartialEq, Eq)]
enum Piece {
    Kept(String),
    Removed(String),
    Added(String),
}

/// A removed line and the added line that replaced it, as one paragraph: the words both keep
/// once, and what each changed marked. When the kept words do not line up, all of the old
/// line is removed and all of the new one added.
fn merge(removed: &[Span], added: &[Span]) -> Vec<Piece> {
    let kept = |spans: &[Span]| -> Vec<String> {
        spans
            .iter()
            .filter(|s| !s.changed)
            .map(|s| s.text.clone())
            .collect()
    };
    if kept(removed) != kept(added) {
        let whole = |spans: &[Span]| spans.iter().map(|s| s.text.as_str()).collect::<String>();
        return [Piece::Removed(whole(removed)), Piece::Added(whole(added))]
            .into_iter()
            .filter(|piece| !matches!(piece, Piece::Removed(t) | Piece::Added(t) if t.is_empty()))
            .collect();
    }
    let (mut old, mut new) = (removed.iter().peekable(), added.iter().peekable());
    let mut pieces = Vec::new();
    loop {
        while let Some(span) = old.next_if(|s| s.changed) {
            pieces.push(Piece::Removed(span.text.clone()));
        }
        while let Some(span) = new.next_if(|s| s.changed) {
            pieces.push(Piece::Added(span.text.clone()));
        }
        match (old.next(), new.next()) {
            (Some(_), Some(span)) => pieces.push(Piece::Kept(span.text.clone())),
            _ => return pieces,
        }
    }
}

/// The diff's lines as paragraphs. Blank lines and lines left out separate them.
fn paragraphs(lines: &[DiffLine]) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut i = 0;
    let text = |spans: &[Span]| spans.iter().map(|s| s.text.as_str()).collect::<String>();
    while let Some(line) = lines.get(i) {
        i += 1;
        match line {
            DiffLine::Same(text) if !text.trim().is_empty() => {
                out.push(Paragraph::Same(text.clone()));
            }
            DiffLine::Removed(old) => {
                if let Some(DiffLine::Added(new)) = lines.get(i) {
                    i += 1;
                    out.push(Paragraph::Changed(merge(old, new)));
                } else if !text(old).trim().is_empty() {
                    out.push(Paragraph::Changed(vec![Piece::Removed(text(old))]));
                }
            }
            DiffLine::Added(new) if !text(new).trim().is_empty() => {
                out.push(Paragraph::Changed(vec![Piece::Added(text(new))]));
            }
            _ => {}
        }
    }
    out
}

fn word_view(paragraphs: &[Paragraph], only_changed: bool) -> Markup {
    html! {
        div.diff-body {
            @for paragraph in paragraphs {
                @match paragraph {
                    Paragraph::Same(text) => @if !only_changed { p.same { (text) } },
                    Paragraph::Changed(pieces) => p.changed {
                        @for (i, piece) in pieces.iter().enumerate() {
                            @let gap = i > 0 && matches!((&pieces[i - 1], piece), (Piece::Removed(_), Piece::Added(_)));
                            @if gap { " " }
                            @match piece {
                                Piece::Kept(text) => (text),
                                Piece::Removed(text) => del { (text) },
                                Piece::Added(text) => ins { (text) },
                            }
                        }
                    },
                }
            }
        }
    }
}

/// One row of the line view: the old and new line numbers, the sign and the text.
struct Row {
    old: Option<u64>,
    new: Option<u64>,
    sign: &'static str,
    class: Option<&'static str>,
    text: String,
}

fn rows(lines: &[DiffLine]) -> Vec<Row> {
    let (mut old, mut new) = (1_u64, 1_u64);
    let text = |spans: &[Span]| spans.iter().map(|s| s.text.as_str()).collect::<String>();
    let mut rows = Vec::new();
    for line in lines {
        match line {
            DiffLine::Skipped(count) => {
                old += count;
                new += count;
            }
            DiffLine::Same(same) => {
                rows.push(Row {
                    old: Some(old),
                    new: Some(new),
                    sign: " ",
                    class: None,
                    text: same.clone(),
                });
                old += 1;
                new += 1;
            }
            DiffLine::Removed(spans) => {
                rows.push(Row {
                    old: Some(old),
                    new: None,
                    sign: "−",
                    class: Some("rem"),
                    text: text(spans),
                });
                old += 1;
            }
            DiffLine::Added(spans) => {
                rows.push(Row {
                    old: None,
                    new: Some(new),
                    sign: "+",
                    class: Some("add"),
                    text: text(spans),
                });
                new += 1;
            }
        }
    }
    rows
}

fn line_view(lines: &[DiffLine]) -> Markup {
    html! {
        ol.line-diff aria-label="Line diff" {
            @for row in rows(lines) {
                li class=[row.class] {
                    span.n { @if let Some(n) = row.old { (n) } }
                    span.n { @if let Some(n) = row.new { (n) } }
                    span.sg { (row.sign) }
                    span { (row.text) }
                }
            }
        }
    }
}

/// `v22, 9 Nov, blackbox`.
fn label(shell: &Shell, event: &Event, n: Option<u64>) -> String {
    let version = n.map_or_else(|| event.commit.short().to_owned(), |n| format!("v{n}"));
    format!(
        "{version}, {}, {}",
        shell.local().date_short(event.time),
        event.author
    )
}

/// The versions a picker offers: the newest, and the chosen one if it is older.
fn options(shell: &Shell, all: &Events, chosen: Option<&CommitId>) -> Markup {
    let mut events: Vec<&Event> = all.events.iter().take(CHOICES).collect();
    if let Some(index) = chosen.and_then(|c| all.events.iter().position(|e| &e.commit == c))
        && index >= CHOICES
    {
        events.push(&all.events[index]);
    }
    html! {
        @for event in events {
            option value=(event.commit) selected[Some(&event.commit) == chosen] {
                (label(shell, event, number_of(all, &event.commit)))
            }
        }
    }
}

/// The diff's heading, the page and who saved it, the words changed, and the switch to show
/// only changed paragraphs.
#[expect(
    clippy::too_many_arguments,
    reason = "one template, each argument one fact on it"
)]
fn head(
    heading: &str,
    base: &str,
    title: &str,
    which: &Markup,
    (added, removed): (u64, u64),
    changed: usize,
    toggle: &str,
    only_changed: bool,
) -> Markup {
    html! {
        header.diff-head {
            h2 #diff-h .rail-h { (heading) }
            p.diff-which { a href=(base) data-carry { (title) } (which) }
            p.diff-sum { (delta(added, removed)) " words, in " (changed) (if changed == 1 { " paragraph" } else { " paragraphs" }) }
            div.diff-tools {
                a.btn href=(toggle) data-only-changes aria-pressed=(only_changed) { "Only changed paragraphs" }
            }
        }
    }
}

/// The form that picks two versions to compare, and the step to the pair before.
fn pick(base: &str, from: &Markup, to: &Markup, step: Option<&(String, String)>) -> Markup {
    html! {
        aside.diff-pick.surface data-surface aria-labelledby="pick-h" {
            h2 #pick-h .rail-h { "Compare" }
            form.pick-form action=(format!("{base}/diff")) method="get" {
                div.field-row { label for="from" { "From" } select.select #from name="from" { (from) } }
                div.field-row { label for="to" { "To" } select.select #to name="to" { (to) } }
                p { button.btn type="submit" { "Compare" } }
            }
            @if let Some((to, text)) = step {
                p.diff-sum { "Or step: " a href=(to) { (text) } }
            }
        }
    }
}

/// `Version 22 to 23`, or commits where a version has no number.
fn heading(all: &Events, from: Option<&CommitId>, to: &CommitId) -> String {
    let name = |commit: Option<&CommitId>| match commit {
        None => "nothing".to_owned(),
        Some(commit) => {
            number_of(all, commit).map_or_else(|| commit.short().to_owned(), |n| n.to_string())
        }
    };
    format!("Version {} to {}", name(from), name(Some(to)))
}

/// The pair of versions just before `from`, to step back through the history.
fn step(base: &str, all: &Events, from: Option<&CommitId>) -> Option<(String, String)> {
    let from = from?;
    let n = number_of(all, from)?;
    let before = n.checked_sub(1).filter(|before| *before > 0)?;
    let older = all
        .events
        .iter()
        .find(|e| number_of(all, &e.commit) == Some(before))?;
    Some((
        href(
            &format!("{base}/diff"),
            &[("from", older.commit.as_str()), ("to", from.as_str())],
        ),
        format!("v{before} to v{n}"),
    ))
}

async fn compare(
    shell: &Shell,
    mneme: &Mneme,
    id: &str,
    query: Result<Query<Params>, QueryRejection>,
) -> Result<Page, Failure> {
    let id = page_id(id)?;
    let params = params(query)?;
    let from: Option<CommitId> = choice(params.from.as_deref())?;
    let to: Option<CommitId> = choice(params.to.as_deref())?;
    let only_changed = params.only.as_deref() == Some("changed");
    let (detail, diff, all, _) = tokio::try_join!(
        mneme.page(&id),
        mneme.diff(&id, from.as_ref(), to.as_ref()),
        all_events(mneme, &id),
        mneme.status(),
    )?;
    let local = shell.local();
    let page = &detail.page;
    let base = page_href(&id);
    let to_event = all.events.iter().find(|event| event.commit == diff.to);
    let heading = heading(&all, diff.from.as_ref(), &diff.to);
    // The words of every version after `from` up to `to`.
    let between: Vec<&Event> = all
        .events
        .iter()
        .skip_while(|event| event.commit != diff.to)
        .take_while(|event| Some(&event.commit) != diff.from.as_ref())
        .collect();
    let (added, removed) = between
        .iter()
        .fold((0, 0), |(a, r), event| (a + event.added, r + event.removed));
    let losers = losers(mneme, &all.events).await;
    let paragraphs = diff.lines.as_deref().map_or_else(Vec::new, paragraphs);
    let changed = paragraphs
        .iter()
        .filter(|p| matches!(p, Paragraph::Changed(_)))
        .count();
    let here = |only: bool| {
        href(
            &format!("{base}/diff"),
            &[
                ("from", diff.from.as_ref().map_or("", CommitId::as_str)),
                ("to", diff.to.as_str()),
                ("only", if only { "changed" } else { "" }),
            ],
        )
    };
    let step = step(&base, &all, diff.from.as_ref());
    let which = html! {
        @if let Some(event) = to_event {
            ", saved " (local.when_long(event.time)) " by " (event.author) ". "
            @if let Some(commit) = &event.overlap {
                @let who = losers.get(commit).map_or_else(|| "Another edit".to_owned(), |l| l.author.clone());
                (overlap(&html! { (who) " edited the same lines at the same time." }))
            }
        }
    };
    let body = html! {
        main #(MAIN) .diffs {
            header.page-head {
                p.crumbs {
                    a href="/wiki" { "Wiki" } " " span aria-hidden="true" { "/" } " "
                    a href=(base) { (page.title) } " " span aria-hidden="true" { "/" } " diff"
                }
                h1 { "What changed" }
            }
            section #diff .diff.surface data-surface aria-labelledby="diff-h" data-module="diff" {
                (head(&heading, &base, &page.title, &which, (added, removed), changed, &here(!only_changed), only_changed))
                @if diff.lines.is_some() {
                    (word_view(&paragraphs, only_changed))
                    details.lines {
                        summary { "Lines" }
                        (line_view(diff.lines.as_deref().unwrap_or_default()))
                    }
                } @else {
                    div.diff-body { p { "These texts are too long to compare." } }
                }
            }
            (pick(&base, &options(shell, &all, diff.from.as_ref()), &options(shell, &all, Some(&diff.to)), step.as_ref()))
        }
    };
    Ok(Page::new(
        format!("What changed in {}, Atrium wiki", page.title),
        "Words added and removed between two versions.",
        body,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, changed: bool) -> Span {
        Span {
            text: text.to_owned(),
            changed,
        }
    }

    #[test]
    fn a_replaced_line_is_one_paragraph_with_its_words_marked() {
        let old = [
            span("read at ", false),
            span("07:00", true),
            span(".", false),
        ];
        let new = [
            span("read at ", false),
            span("06:00", true),
            span(".", false),
        ];
        assert_eq!(
            merge(&old, &new),
            [
                Piece::Kept("read at ".into()),
                Piece::Removed("07:00".into()),
                Piece::Added("06:00".into()),
                Piece::Kept(".".into())
            ]
        );
    }

    #[test]
    fn lines_that_do_not_line_up_are_removed_and_added_whole() {
        let old = [span("a ", false), span("b", true)];
        let new = [span("c ", false), span("d", true)];
        assert_eq!(
            merge(&old, &new),
            [Piece::Removed("a b".into()), Piece::Added("c d".into())]
        );
    }

    #[test]
    fn blank_lines_and_skips_are_not_paragraphs() {
        let lines = [
            DiffLine::Skipped(3),
            DiffLine::Same("kept".into()),
            DiffLine::Same(String::new()),
            DiffLine::Added(vec![span("new", true)]),
            DiffLine::Added(Vec::new()),
        ];
        assert_eq!(
            paragraphs(&lines),
            [
                Paragraph::Same("kept".into()),
                Paragraph::Changed(vec![Piece::Added("new".into())])
            ]
        );
    }
}
