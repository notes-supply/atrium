//! The wiki's part of home, the prototype's `home` and `awayView`: clauses of the opening
//! sentence, the map, and the tiles for recent changes, waiting questions, the reading slot
//! and the week. Everything is fetched at once against the home page's deadline; a part that
//! misses it gives way to a quiet tile and its clause is left out.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::time::Instant;

use atrium_core::{HomeContext, HomeOffer, Local};
use atrium_ui::components::{Mark, Want, away_tile, mark, since, tile_head};
use atrium_ui::draw::{self, Day};
use atrium_ui::words::{cap, count};
use atrium_ui::{Page, Shell};
use maud::{Markup, html};

use crate::machines::{Copy, Known, judge, map_figure};
use crate::mneme::{Events, Machines, Mneme, MnemeError, PageId, Questions};
use crate::routes::{Loser, RowLook, change_row, losers, page_href, title_of, titles};

/// Changes the tile lists.
const TILE_CHANGES: usize = 8;
/// Events the week reads, at most.
const WEEK_MOST: u64 = 2000;
const DAY: i64 = 24 * 60 * 60;

/// What the opening sentence is made of. `None` is a fact Atrium could not learn in time.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// When the reader was last here, if Atrium knows.
    pub last: Option<i64>,
    pub changes: Option<ChangeFacts>,
    pub machines: Option<Vec<(String, Want, String)>>,
    /// Open questions, `None` also when Mneme does not list them.
    pub questions: Option<u64>,
}

/// What changed: how many pages since the last visit, and when the newest change was.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChangeFacts {
    pub pages_since: u64,
    pub newest: Option<i64>,
}

/// `a, b, and c`: each machine's clause, the last after "and".
fn listed(parts: Vec<Markup>) -> Markup {
    let n = parts.len();
    html! {
        @for (i, part) in parts.into_iter().enumerate() {
            @if i > 0 { ", " }
            @if i > 0 && i + 1 == n { "and " }
            (part)
        }
    }
}

/// The opening sentence after the greeting, one clause per fact Atrium has: what changed
/// since the last visit, which machines want you, and how many questions wait.
#[must_use]
pub fn sentence(local: &Local, facts: &Facts) -> Vec<Markup> {
    let mut said = Vec::new();
    if let Some(changes) = facts.changes {
        said.push(match (facts.last, changes.pages_since, changes.newest) {
            (Some(last), 0, _) => html! { "Nothing changed since you were last here at " (local.time(last, &local.hm(last))) "." },
            (Some(last), pages, newest) => html! {
                a href="/wiki/changes#new" { (cap(&count(pages, "page", "pages"))) " changed" }
                " since you were last here at " (local.time(last, &local.hm(last)))
                @if let Some(newest) = newest { ", the newest " (local.ago(newest)) }
                "."
            },
            (None, _, Some(newest)) => html! { a href="/wiki/changes" { "The newest change" } " was " (local.ago(newest)) "." },
            (None, _, None) => html! { "The wiki has no changes yet." },
        });
    }
    if let Some(machines) = &facts.machines {
        let mut hot: Vec<&(String, Want, String)> = machines
            .iter()
            .filter(|(_, want, _)| *want != Want::Rest)
            .collect();
        hot.sort_by_key(|(_, want, _)| std::cmp::Reverse(*want));
        if hot.is_empty() {
            match machines.len() {
                0 => {}
                1 => said.push(html! { "The folder matches the record." }),
                2 => said.push(html! { "Both folders match the record." }),
                n => said.push(html! { "All " (n) " folders match the record." }),
            }
        } else {
            let parts = hot
                .iter()
                .map(|(name, want, says)| html! {
                    a.hot data-want=(want.level()) href={ "/wiki/machines#" (name) } { (mark(Mark::Want(*want))) (name) " " (says) }
                })
                .collect();
            said.push(html! { (listed(parts)) "." });
        }
    }
    if let Some(questions @ 1..) = facts.questions {
        said.push(html! { a href="#waiting" { (cap(&count(questions, "question is", "questions are"))) " waiting for you." } });
    }
    said
}

/// Versions per hour over the last seven days of the reader's calendar, today last.
fn week(local: &Local, events: &Events) -> draw::Week {
    let today = local.now().date();
    let now_hour = usize::from(local.hour());
    let mut days: Vec<Day> = (0..7)
        .rev()
        .map(|ago: i64| {
            let date = today
                .checked_sub(jiff::Span::new().days(ago))
                .unwrap_or(today);
            let noon = local
                .now()
                .with()
                .date(date)
                .hour(12)
                .build()
                .map_or(local.seconds(), |z| z.timestamp().as_second());
            let mut hours = [Some(0); 24];
            if ago == 0 {
                hours
                    .iter_mut()
                    .skip(now_hour + 1)
                    .for_each(|hour| *hour = None);
            }
            Day {
                name: local.weekday_short(noon).to_owned(),
                hours,
            }
        })
        .collect();
    for event in events.events.iter().filter(|event| event.version) {
        let at = local.at(event.time);
        let ago = local.days_ago(event.time);
        let day = usize::try_from(6 - ago).ok().and_then(|i| days.get_mut(i));
        let hour = usize::try_from(at.hour()).ok();
        if let Some(Some(count)) = day
            .zip(hour)
            .and_then(|(day, hour)| day.hours.get_mut(hour))
        {
            *count += 1;
        }
    }
    draw::week(&days, now_hour)
}

fn week_tile(local: &Local, events: &Events) -> Markup {
    let week = week(local, events);
    let note = match week.busiest {
        Some((6, hour)) => format!("The busiest hour was today at {hour:02}:00."),
        Some((day, hour)) => {
            let ago = i64::try_from(6 - day).unwrap_or(0);
            format!(
                "The busiest hour was {} at {hour:02}:00.",
                local.weekday(local.seconds() - ago * DAY)
            )
        }
        None => "Nothing was saved this week.".to_owned(),
    };
    html! {
        article.tile.week.surface data-surface aria-labelledby="week-h" {
            (tile_head("week-h", "This week", Some(html! { span.more-n { (count(week.total, "version", "versions")) } })))
            (week.svg)
            p.week-note { (note) }
        }
    }
}

fn changes_tile(
    local: &Local,
    last: Option<i64>,
    recent: &Events,
    titles: &HashMap<PageId, String>,
    losers: &HashMap<String, Loser>,
) -> Markup {
    let new = recent
        .events
        .iter()
        .filter(|event| last.is_some_and(|last| event.time > last))
        .count();
    let look = RowLook {
        day: true,
        diff_link: false,
    };
    html! {
        article.tile.changes.surface data-surface aria-labelledby="changes-h" {
            header.tile-head {
                h2 #changes-h { "Recent changes" }
                @if let Some(last) = last {
                    (since(&count(u64::try_from(new).unwrap_or(0), "new", "new"), local, last))
                }
            }
            ol.change-list {
                @for event in recent.events.iter().take(TILE_CHANGES) {
                    (change_row(local, last, event, title_of(titles, event), losers, look))
                }
            }
            a.more href="/wiki/changes" { "All changes" }
        }
    }
}

fn waiting_tile(local: &Local, questions: &Questions) -> Markup {
    html! {
        article #waiting .tile.waiting.surface data-surface data-want="1" aria-labelledby="waiting-h" {
            (tile_head("waiting-h", "Waiting for you", Some(html! { span.more-n { (count(questions.total, "question", "questions")) } })))
            ol.q-list {
                @for question in &questions.questions {
                    li {
                        (mark(Mark::Want(Want::Needs)))
                        p.q-text { a href={ (page_href(&question.page.id)) "#open-questions" } data-row { (question.text) } }
                        p.q-on { "on " (question.page.title) ", updated " (local.when(question.updated)) }
                    }
                }
            }
        }
    }
}

/// Where the reading module puts "Continue reading". The browser keeps the positions, so
/// without scripts the tile stays hidden.
fn resume_slot() -> Markup {
    html! {
        article.tile.resume.surface data-surface data-module="continue" aria-labelledby="resume-h" hidden {
            (tile_head("resume-h", "Continue reading", None))
        }
    }
}

fn away_note(retry: &str) -> Markup {
    html! {
        article.tile.away-note.surface data-surface aria-labelledby="away-h" {
            (tile_head("away-h", "What still works", None))
            p { "Your place in each page is kept in this browser, so the list beside this still knows where you were. Search, history and the machines come back with the wiki." }
            p { a.btn href=(retry) { "Try again" } }
        }
    }
}

/// The wiki's part of home when Mneme does not answer at all.
fn away(local: &Local, heard: Option<i64>, retry: &str) -> HomeOffer {
    HomeOffer {
        said: vec![html! {
            "The wiki is out of reach just now. "
            @match heard {
                Some(heard) => { "Atrium last heard from it at " (local.time(heard, &local.hm(heard))) }
                None => "Atrium has not heard from it since it started,",
            }
            " and will try again when you load a page."
        }],
        figure: Some(map_figure(local, &Known::Away { heard }, "The wiki")),
        tiles: vec![away_note(retry), resume_slot()],
        away: true,
    }
}

/// The page every wiki route shows when Mneme does not answer: home, as it looks while the
/// wiki is away, with a 503.
#[must_use]
pub fn away_page(shell: &Shell, mneme: &Mneme) -> Page {
    let offer = away(shell.local(), mneme.heard(), shell.path());
    atrium_ui::home(shell, &[("Wiki", offer)])
}

/// A call that gives up at the deadline, as away.
async fn by<T>(
    deadline: Instant,
    call: impl Future<Output = Result<T, MnemeError>>,
) -> Result<T, MnemeError> {
    tokio::time::timeout_at(deadline.into(), call)
        .await
        .unwrap_or_else(|_| Err(MnemeError::Away("over the home page's budget".to_owned())))
}

/// Every event since `since`, newest first, up to [`WEEK_MOST`].
async fn changes_since(mneme: &Mneme, since: i64) -> Result<Events, MnemeError> {
    let mut all = mneme.changes(0, 500, Some(since)).await?;
    while u64::try_from(all.events.len()).unwrap_or(u64::MAX) < all.total.min(WEEK_MOST) {
        let more = mneme
            .changes(
                u64::try_from(all.events.len()).unwrap_or(0),
                500,
                Some(since),
            )
            .await?;
        if more.events.is_empty() {
            break;
        }
        all.events.extend(more.events);
    }
    Ok(all)
}

/// How many pages changed since the last visit, from the week's events and the newest few,
/// and when the newest change was.
fn change_facts(
    last: Option<i64>,
    recent: Option<&Events>,
    week: Option<&Events>,
) -> Option<ChangeFacts> {
    let recent = recent?;
    let pages: HashSet<&PageId> = week
        .map_or(&recent.events, |week| &week.events)
        .iter()
        .chain(&recent.events)
        .filter(|event| last.is_some_and(|last| event.time > last))
        .map(|event| &event.page)
        .collect();
    Some(ChangeFacts {
        pages_since: u64::try_from(pages.len()).unwrap_or(0),
        newest: recent.events.first().map(|event| event.time),
    })
}

/// Each machine's name, how much it wants you, and what to say of it.
fn machine_facts(local: &Local, copies: &[Copy]) -> Vec<(String, Want, String)> {
    copies
        .iter()
        .map(|copy| {
            let says = copy
                .concerns
                .first()
                .map_or_else(String::new, |c| c.says(local));
            (copy.name.clone(), copy.want(), says)
        })
        .collect()
}

pub async fn offer(mneme: &Mneme, context: &HomeContext) -> HomeOffer {
    let local = &context.visit.local;
    let last = context.visit.last;
    let deadline = context.deadline;
    let week_start = local
        .now()
        .start_of_day()
        .map_or(local.seconds() - 7 * DAY, |start| {
            start.timestamp().as_second() - 6 * DAY
        });
    let (recent, week_events, titles, machines, status, questions) = tokio::join!(
        by(
            deadline,
            mneme.changes(0, u64::try_from(TILE_CHANGES).unwrap_or(8), None)
        ),
        by(deadline, changes_since(mneme, week_start)),
        by(deadline, titles(mneme)),
        by(deadline, mneme.machines()),
        by(deadline, mneme.status()),
        by(deadline, mneme.questions()),
    );
    let all_away = [
        recent.is_err(),
        week_events.is_err(),
        titles.is_err(),
        machines.is_err(),
        status.is_err(),
        questions.is_err(),
    ]
    .iter()
    .all(|failed| *failed);
    if all_away {
        return away(local, mneme.heard(), "/");
    }
    let losers = match &recent {
        Ok(recent) => losers(mneme, &recent.events).await,
        Err(_) => HashMap::new(),
    };
    let copies: Option<Vec<Copy>> = machines.as_ref().ok().map(|found: &Machines| {
        found
            .machines
            .iter()
            .map(|m| judge(m, status.as_ref().ok(), local.seconds()))
            .collect()
    });
    let facts = Facts {
        last,
        changes: change_facts(last, recent.as_ref().ok(), week_events.as_ref().ok()),
        machines: copies.as_ref().map(|copies| machine_facts(local, copies)),
        questions: questions
            .as_ref()
            .ok()
            .and_then(|found| found.as_ref().map(|q| q.total)),
    };
    let known = match &copies {
        Some(copies) => Known::Known {
            copies,
            record: status.as_ref().ok(),
        },
        None => Known::Away {
            heard: mneme.heard(),
        },
    };
    let figure = map_figure(local, &known, "The wiki");
    let no_titles = HashMap::new();
    let mut shown = vec![match &recent {
        Ok(recent) => changes_tile(
            local,
            last,
            recent,
            titles.as_ref().unwrap_or(&no_titles),
            &losers,
        ),
        Err(_) => away_tile("changes", "Recent changes"),
    }];
    match &questions {
        Ok(Some(found)) if found.total > 0 => shown.push(waiting_tile(local, found)),
        Ok(_) => {}
        Err(_) => shown.push(away_tile("waiting", "Waiting for you")),
    }
    shown.push(resume_slot());
    shown.push(match &week_events {
        Ok(events) => week_tile(local, events),
        Err(_) => away_tile("week", "This week"),
    });
    HomeOffer {
        said: sentence(local, &facts),
        figure: Some(figure),
        tiles: shown,
        away: false,
    }
}

#[cfg(test)]
mod tests {
    use jiff::tz::TimeZone;

    use super::*;

    fn local() -> Local {
        Local::new(
            "2026-11-11T08:41:00Z".parse().unwrap(),
            TimeZone::get("Europe/Stockholm").unwrap(),
        )
    }

    fn said(facts: &Facts) -> String {
        let local = local();
        let html = sentence(&local, facts)
            .into_iter()
            .map(Markup::into_string)
            .collect::<Vec<_>>()
            .join(" ");
        scraper::Html::parse_fragment(&html)
            .root_element()
            .text()
            .collect::<String>()
    }

    const LAST: i64 = 1_794_379_680;
    const NEWEST: i64 = 1_794_384_720;

    #[test]
    fn nothing_to_report() {
        let facts = Facts {
            last: Some(LAST),
            changes: Some(ChangeFacts {
                pages_since: 0,
                newest: Some(LAST - 600),
            }),
            machines: Some(vec![
                ("ironbark".into(), Want::Rest, String::new()),
                ("willow".into(), Want::Rest, String::new()),
            ]),
            questions: Some(0),
        };
        assert_eq!(
            said(&facts),
            "Nothing changed since you were last here at 07:48. Both folders match the record."
        );
    }

    #[test]
    fn says_how_many_pages_changed_since_the_last_visit() {
        let facts = Facts {
            last: Some(LAST),
            changes: Some(ChangeFacts {
                pages_since: 5,
                newest: Some(NEWEST),
            }),
            ..Facts::default()
        };
        assert_eq!(
            said(&facts),
            "Five pages changed since you were last here at 07:48, the newest 29 minutes ago."
        );
    }

    #[test]
    fn without_a_last_visit_says_when_the_newest_change_was() {
        let facts = Facts {
            changes: Some(ChangeFacts {
                pages_since: 0,
                newest: Some(NEWEST),
            }),
            ..Facts::default()
        };
        assert_eq!(said(&facts), "The newest change was 29 minutes ago.");
    }

    #[test]
    fn names_the_machines_that_want_you_strongest_first() {
        let facts = Facts {
            machines: Some(vec![
                (
                    "ironbark".into(),
                    Want::Look,
                    "left one file out of a save".into(),
                ),
                ("willow".into(), Want::Needs, "is blocked".into()),
            ]),
            ..Facts::default()
        };
        assert_eq!(
            said(&facts),
            "willow is blocked, and ironbark left one file out of a save."
        );
        let one = Facts {
            machines: Some(vec![("willow".into(), Want::Needs, "is behind".into())]),
            ..Facts::default()
        };
        assert_eq!(said(&one), "willow is behind.");
    }

    #[test]
    fn counts_the_questions_waiting() {
        assert_eq!(
            said(&Facts {
                questions: Some(3),
                ..Facts::default()
            }),
            "Three questions are waiting for you."
        );
        assert_eq!(
            said(&Facts {
                questions: Some(1),
                ..Facts::default()
            }),
            "One question is waiting for you."
        );
        assert_eq!(
            said(&Facts {
                questions: None,
                ..Facts::default()
            }),
            ""
        );
    }

    #[test]
    fn leaves_out_what_it_could_not_learn() {
        assert_eq!(
            said(&Facts {
                last: Some(LAST),
                ..Facts::default()
            }),
            ""
        );
    }
}
