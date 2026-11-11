//! The machines that hold a copy of the wiki and the record they push to: how much each wants
//! you, the map, the cards under it, and `/wiki/machines`, the prototype's `machinesView`.

use std::fmt::Write;

use atrium_core::Local;
use atrium_ui::components::{Mark, Want, mark};
use atrium_ui::draw::{self, Place};
use atrium_ui::words::{cap, count, number};
use atrium_ui::{MAIN, Page, Shell};
use axum::extract::State;
use axum::response::Response;
use maud::{Markup, html};

use crate::RECORD;
use crate::mneme::{Machine, Mneme, Skipped, Status};
use crate::routes::{Failure, respond};

/// How long a machine may go unseen, or a copy stay behind, before it needs you.
const PATIENCE: i64 = 10 * 60;

/// Why a machine wants you, the strongest reason first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Concern {
    /// The folder is in a state the service will not touch, or the record refused a push.
    Blocked(String),
    /// A save would have deleted most pages, so nothing was saved.
    Stopped(String),
    /// Not seen for over ten minutes, or not since the record started (`None`).
    Unseen(Option<i64>),
    /// Behind the record for over ten minutes, since this time.
    Behind(i64),
    /// The record cannot be reached, with Git's message.
    Offline(String),
    /// Files kept out of every save.
    Skipped(u64),
}

impl Concern {
    #[must_use]
    pub const fn want(&self) -> Want {
        match self {
            Self::Blocked(_) | Self::Stopped(_) | Self::Unseen(_) | Self::Behind(_) => Want::Needs,
            Self::Offline(_) | Self::Skipped(_) => Want::Look,
        }
    }

    /// The end of a sentence that starts with the machine's name.
    #[must_use]
    pub fn says(&self, local: &Local) -> String {
        match self {
            Self::Blocked(_) => "is blocked".to_owned(),
            Self::Stopped(_) => "is stopped".to_owned(),
            Self::Unseen(Some(seen)) => format!(
                "has not been seen for {}",
                local.ago(*seen).trim_end_matches(" ago")
            ),
            Self::Unseen(None) => "has not been seen".to_owned(),
            Self::Behind(_) => "is behind".to_owned(),
            Self::Offline(_) => "is offline".to_owned(),
            Self::Skipped(files) => {
                format!("left {} out of a save", count(*files, "file", "files"))
            }
        }
    }

    /// Why, in a sentence of its own.
    fn why(&self, local: &Local, state: &str) -> String {
        let given = |detail: &str| {
            if detail.is_empty() {
                meaning(state).to_owned()
            } else {
                detail.to_owned()
            }
        };
        match self {
            Self::Blocked(detail) | Self::Stopped(detail) | Self::Offline(detail) => given(detail),
            Self::Unseen(Some(seen)) => format!("Not seen since {}.", local.when_long(*seen)),
            Self::Unseen(None) => "Not seen since the wiki's server started.".to_owned(),
            Self::Behind(since) => format!("Behind the record since {}.", local.hm(*since)),
            Self::Skipped(files) => {
                format!("Left {} out of a save.", count(*files, "file", "files"))
            }
        }
    }
}

/// What each state Mneme reports means.
const STATES: [(&str, Want, &str); 7] = [
    (
        "idle",
        Want::Rest,
        "Nothing to do. The copy matches its folder.",
    ),
    (
        "saved",
        Want::Rest,
        "Saved local edits as a commit, not yet pushed.",
    ),
    ("pulled", Want::Rest, "Took new versions from the record."),
    ("pushed", Want::Rest, "Sent its versions to the record."),
    (
        "offline",
        Want::Look,
        "Cannot reach the record. Git's message says why.",
    ),
    (
        "blocked",
        Want::Needs,
        "Waits on its folder, or the record refused a push. Changes nothing until a person looks.",
    ),
    (
        "stopped",
        Want::Needs,
        "Refused a save that would have deleted most pages. Nothing was saved.",
    ),
];

fn meaning(state: &str) -> &'static str {
    STATES
        .iter()
        .find(|(name, ..)| *name == state)
        .map_or("", |(.., says)| *says)
}

/// One machine as the pages show it.
#[derive(Debug, Clone)]
pub struct Copy {
    pub name: String,
    pub state: String,
    pub detail: String,
    pub seen: Option<i64>,
    pub current: bool,
    pub skipped: Vec<Skipped>,
    pub skipped_more: u64,
    /// Every reason it wants you, the strongest first.
    pub concerns: Vec<Concern>,
}

impl Copy {
    pub fn want(&self) -> Want {
        self.concerns.first().map_or(Want::Rest, Concern::want)
    }

    /// Its state as the pages name it.
    #[must_use]
    pub fn state_word(&self) -> &str {
        if self.state.is_empty() {
            "not reported"
        } else {
            &self.state
        }
    }

    /// Why it wants you, or what its state means when it does not.
    #[must_use]
    pub fn why(&self, local: &Local) -> String {
        match self.concerns.first() {
            Some(concern) => concern.why(local, &self.state),
            None if self.detail.is_empty() => meaning(&self.state).to_owned(),
            None => self.detail.clone(),
        }
    }

    #[must_use]
    pub fn left_out(&self) -> u64 {
        u64::try_from(self.skipped.len()).unwrap_or(0) + self.skipped_more
    }
}

/// Reads what Mneme says of a machine against the clock and the record. A machine needs you
/// when it is blocked or stopped, unseen for over ten minutes, or behind the record for over
/// ten minutes; it is worth a look when it is offline or keeps files out of its saves.
#[must_use]
pub fn judge(machine: &Machine, record: Option<&Status>, now: i64) -> Copy {
    let state = machine.state.clone().unwrap_or_default();
    let detail = machine.detail.clone().unwrap_or_default();
    let skipped = machine.skipped.clone().unwrap_or_default();
    let skipped_more = machine.skipped_more.unwrap_or(0);
    let mut concerns = Vec::new();
    match state.as_str() {
        "blocked" => concerns.push(Concern::Blocked(detail.clone())),
        "stopped" => concerns.push(Concern::Stopped(detail.clone())),
        _ => {}
    }
    match machine.seen {
        None => concerns.push(Concern::Unseen(None)),
        Some(seen) if now - seen > PATIENCE => concerns.push(Concern::Unseen(Some(seen))),
        Some(_) => {}
    }
    // Mneme says when the copy stopped matching. Without that, the record's head is newer
    // than anything this machine reported, so it has been behind at least since the head
    // was made.
    let behind = machine
        .behind_since
        .or_else(|| record.and_then(|status| status.head_time));
    if let Some(since) = behind.filter(|_| !machine.current)
        && now - since > PATIENCE
    {
        concerns.push(Concern::Behind(since));
    }
    if state == "offline" {
        concerns.push(Concern::Offline(detail.clone()));
    }
    let left_out = u64::try_from(skipped.len()).unwrap_or(0) + skipped_more;
    if left_out > 0 {
        concerns.push(Concern::Skipped(left_out));
    }
    concerns.sort_by_key(|concern| std::cmp::Reverse(concern.want()));
    Copy {
        name: machine.name.clone(),
        state,
        detail,
        seen: machine.seen,
        current: machine.current,
        skipped,
        skipped_more,
        concerns,
    }
}

/// Which drawn place each machine takes: its own when the map has one by its name, otherwise
/// the first one left. A machine beyond the two is not on the map.
#[must_use]
pub fn places(copies: &[Copy]) -> Vec<(Place, Option<&Copy>)> {
    let folders = [Place::Blackbox, Place::Aeolus];
    let mut taken: Vec<(Place, Option<&Copy>)> =
        folders.iter().map(|place| (*place, None)).collect();
    let mut rest = Vec::new();
    for copy in copies {
        match taken
            .iter_mut()
            .find(|(place, held)| place.name() == copy.name && held.is_none())
        {
            Some(slot) => slot.1 = Some(copy),
            None => rest.push(copy),
        }
    }
    for copy in rest {
        if let Some(slot) = taken.iter_mut().find(|(_, held)| held.is_none()) {
            slot.1 = Some(copy);
        }
    }
    taken
}

/// The drawn place a machine is on the machines page: its place on the map, or the low box.
fn place_of(copies: &[Copy], name: &str) -> Place {
    places(copies)
        .into_iter()
        .find(|(_, copy)| copy.is_some_and(|copy| copy.name == name))
        .map_or(Place::Aeolus, |(place, _)| place)
}

fn names(list: &[&str]) -> String {
    match list {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., final_one] => format!("{} and {final_one}", rest.join(", ")),
    }
}

/// What a screen reader hears for the map.
fn map_title(copies: &[Copy]) -> String {
    let held: Vec<&str> = copies.iter().map(|copy| copy.name.as_str()).collect();
    let holds = if held.len() == 1 {
        "holds a folder"
    } else {
        "each hold a folder"
    };
    let mut title = format!(
        "The map of the wiki: {} {holds}, {RECORD} holds the record.",
        names(&held)
    );
    let needs: Vec<&str> = copies
        .iter()
        .filter(|c| c.want() == Want::Needs)
        .map(|c| c.name.as_str())
        .collect();
    let look: Vec<&str> = copies
        .iter()
        .filter(|c| c.want() == Want::Look)
        .map(|c| c.name.as_str())
        .collect();
    let one = |list: &[&str], single: &'static str, plural: &'static str| {
        if list.len() == 1 { single } else { plural }
    };
    if !needs.is_empty() {
        let _ = write!(
            title,
            " {} {} you and {} drawn in full colour.",
            names(&needs),
            one(&needs, "needs", "need"),
            one(&needs, "is", "are")
        );
    }
    if !look.is_empty() {
        let _ = write!(
            title,
            " {} {} worth a look and {} drawn half resolved.",
            names(&look),
            one(&look, "is", "are"),
            one(&look, "is", "are")
        );
    }
    title
}

/// What the map knows: each machine and the record, or nothing when the wiki is away.
pub enum Known<'a> {
    Known {
        copies: &'a [Copy],
        record: Option<&'a Status>,
    },
    /// The wiki is away; Atrium last heard from it then, if ever.
    Away { heard: Option<i64> },
}

fn dock_card(local: &Local, place: Place, copy: Option<&Copy>, known: &Known<'_>) -> Markup {
    let want = copy.map_or(Want::Rest, Copy::want);
    let name = copy.map_or(place.name(), |copy| copy.name.as_str());
    html! {
        a.dock-card.surface #{ "dock-" (name) } data-m=(place.name()) data-want=(want.level())
            href={ "/wiki/machines#" (name) } data-surface data-want-field=[(want != Want::Rest).then_some(want.level())]
        {
            (draw::place(place, want, "vignette"))
            span.dk-head { span.dk-tone aria-hidden="true" {} span.dk-name { (name) } span.dk-role { "a folder" } }
            @match (known, copy) {
                (Known::Away { .. }, _) => {
                    span.dk-state { (mark(Mark::Want(Want::Rest))) "unknown" }
                    span.dk-why { "Its state comes from the wiki." }
                },
                (Known::Known { .. }, Some(copy)) => {
                    span.dk-state { (mark(Mark::Want(want))) (copy.state_word()) }
                    span.dk-why { (copy.why(local)) }
                    dl.dk-data {
                        dt { "seen" } dd { @match copy.seen { Some(seen) => (local.time(seen, &local.ago(seen))), None => "never" } }
                        dt { "copy" } dd { (if copy.current { "current" } else { "behind" }) }
                    }
                },
                (Known::Known { .. }, None) => {
                    span.dk-state { (mark(Mark::Want(Want::Rest))) "none" }
                    span.dk-why { "No machine holds this place." }
                },
            }
        }
    }
}

fn record_card(local: &Local, known: &Known<'_>) -> Markup {
    html! {
        a.dock-card.surface #dock-apollo data-m="apollo" data-want="0" href="/wiki/machines#apollo" data-surface {
            (draw::place(Place::Apollo, Want::Rest, "vignette"))
            span.dk-head { span.dk-tone aria-hidden="true" {} span.dk-name { (RECORD) } span.dk-role { "the record" } }
            @match known {
                Known::Known { record: Some(status), .. } => {
                    span.dk-state { (mark(Mark::Want(Want::Rest))) "holding" }
                    dl.dk-data {
                        dt { "pages" } dd { (number(status.pages)) }
                        dt { "commits" } dd { (number(status.commits)) }
                        @if let Some(newest) = status.head_time { dt { "newest" } dd { (local.hm(newest)) } }
                    }
                },
                Known::Known { record: None, .. } | Known::Away { heard: None } => {
                    span.dk-state { (mark(Mark::Want(Want::Rest))) "out of reach" }
                    span.dk-why { "Not answered yet." }
                },
                Known::Away { heard: Some(heard) } => {
                    span.dk-state { (mark(Mark::Want(Want::Rest))) "out of reach" }
                    span.dk-why { "Not answered since " (local.hm(*heard)) "." }
                },
            }
        }
    }
}

/// The map of the wiki and a card per place below it.
#[must_use]
pub fn map_figure(local: &Local, known: &Known<'_>, heading: &str) -> Markup {
    let none: [Copy; 0] = [];
    let copies = match known {
        Known::Known { copies, .. } => *copies,
        Known::Away { .. } => &none,
    };
    let placed = places(copies);
    let want = |place: Place| {
        placed
            .iter()
            .find(|(p, _)| *p == place)
            .and_then(|(_, copy)| *copy)
            .map_or(Want::Rest, Copy::want)
    };
    let away = matches!(known, Known::Away { .. });
    let title = if away {
        "The map of the wiki, with nothing known: the wiki is out of reach.".to_owned()
    } else {
        map_title(copies)
    };
    let card = |place: Place| {
        let copy = placed
            .iter()
            .find(|(p, _)| *p == place)
            .and_then(|(_, copy)| *copy);
        dock_card(local, place, copy, known)
    };
    html! {
        figure.map.unknown[away] aria-labelledby="map-h" {
            h2 #map-h .map-h.label { (heading) }
            div.map-stage { (draw::map(want, &title)) }
            ul.docks aria-label="Places" {
                li { (card(Place::Blackbox)) }
                li { (record_card(local, known)) }
                li { (card(Place::Aeolus)) }
            }
        }
    }
}

fn legend() -> Markup {
    html! {
        aside.legend.surface data-surface aria-labelledby="legend-h" {
            h2 #legend-h .rail-h { "What the states mean" }
            dl.state-legend {
                @for (state, want, says) in STATES {
                    div { (mark(Mark::Want(want))) dt { (state) } dd { (says) } }
                }
                div { (mark(Mark::Want(Want::Needs))) dt { "behind" } dd { "A copy behind the record, or a machine not seen, for over ten minutes." } }
                div { (mark(Mark::Want(Want::Look))) dt { "left out" } dd { "Files the service keeps out of every save." } }
            }
        }
    }
}

fn machine_section(local: &Local, copy: &Copy, place: Place) -> Markup {
    let want = copy.want();
    html! {
        section.mach.surface #(copy.name) data-surface data-want=(want.level()) aria-labelledby={ "m-" (copy.name) } {
            header.mach-head { h2 #{ "m-" (copy.name) } { (copy.name) } span.dk-role { "holds a folder" } }
            (draw::place(place, want, "vignette"))
            div.mach-state {
                p.word { (mark(Mark::Want(want))) (copy.state_word()) }
                p.why { (copy.why(local)) }
            }
            dl.facts {
                div { dt { "Last seen" } dd { @match copy.seen { Some(seen) => (local.time(seen, &format!("{}, {}", local.hm(seen), local.ago(seen)))), None => "never" } } }
                div { dt { "Copy" } dd { (if copy.current { "matches the record" } else { "behind the record" }) } }
                div { dt { "Left out" } dd { @if copy.left_out() == 0 { "nothing" } @else { (count(copy.left_out(), "file", "files")) } } }
            }
            @if copy.left_out() > 0 {
                div.skips {
                    h3 { "Files left out of a save" }
                    ul {
                        @for skip in &copy.skipped { li { code { (skip.path) } span { (cap(&skip.reason)) "." } } }
                        @if copy.skipped_more > 0 { li { span { "And " (count(copy.skipped_more, "more file", "more files")) "." } } }
                    }
                }
            }
        }
    }
}

pub async fn show(shell: Shell, State(mneme): State<Mneme>) -> Response {
    let shown = machines(&shell, &mneme).await;
    respond(&shell, &mneme, shown)
}

async fn machines(shell: &Shell, mneme: &Mneme) -> Result<Page, Failure> {
    let (found, status) = tokio::try_join!(mneme.machines(), mneme.status())?;
    let local = shell.local();
    let copies: Vec<Copy> = found
        .machines
        .iter()
        .map(|m| judge(m, Some(&status), local.seconds()))
        .collect();
    let known = Known::Known {
        copies: &copies,
        record: Some(&status),
    };
    let held = u64::try_from(copies.len()).unwrap_or(0);
    let body = html! {
        main #(MAIN) .machines {
            header.page-head {
                h1 { "Machines" }
                p.sub {
                    (cap(&count(held, "machine", "machines"))) (if held == 1 { " holds" } else { " hold" })
                    " a folder of the wiki and save it on their own. " (RECORD)
                    " holds the record they push to. A place is drawn in full when it needs you."
                }
            }
            (map_figure(local, &known, "The map"))
            (legend())
            @for copy in &copies { (machine_section(local, copy, place_of(&copies, &copy.name))) }
            section.mach.surface #apollo data-surface data-want="0" aria-labelledby="m-apollo" {
                header.mach-head { h2 #m-apollo { (RECORD) } span.dk-role { "the record" } }
                (draw::place(Place::Apollo, Want::Rest, "vignette"))
                div.mach-state {
                    p.word { (mark(Mark::Want(Want::Rest))) "holding" }
                    p.why { "Every machine pushes here. What Atrium knows of " (RECORD) " is the wiki's own status." }
                }
                dl.facts {
                    div { dt { "Pages" } dd { (number(status.pages)) } }
                    div { dt { "Commits" } dd { (number(status.commits)) } }
                    @if let Some(newest) = status.head_time {
                        div { dt { "Newest" } dd { (local.time(newest, &local.when_long(newest))) } }
                    }
                }
            }
        }
    };
    Ok(Page::new(
        "Machines, Atrium wiki",
        "Each machine that holds a copy of the wiki, and the record.",
        body,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_794_386_460;

    fn machine(state: &str, seen: Option<i64>, current: bool) -> Machine {
        serde_json::from_value(serde_json::json!({
            "name": "aeolus", "state": state, "detail": "", "seen": seen, "current": current,
        }))
        .unwrap()
    }

    fn record(head_time: i64) -> Status {
        serde_json::from_value(
            serde_json::json!({ "pages": 1, "commits": 1, "head_time": head_time }),
        )
        .unwrap()
    }

    #[test]
    fn a_quiet_machine_rests() {
        let copy = judge(
            &machine("idle", Some(NOW - 60), true),
            Some(&record(NOW - 3600)),
            NOW,
        );
        assert_eq!(copy.want(), Want::Rest);
    }

    #[test]
    fn blocked_and_stopped_need_you() {
        for state in ["blocked", "stopped"] {
            let copy = judge(&machine(state, Some(NOW - 60), true), None, NOW);
            assert_eq!(copy.want(), Want::Needs, "{state}");
        }
    }

    #[test]
    fn unseen_for_over_ten_minutes_needs_you() {
        let copy = judge(&machine("idle", Some(NOW - 11 * 60), true), None, NOW);
        assert_eq!(copy.concerns, [Concern::Unseen(Some(NOW - 11 * 60))]);
        assert_eq!(
            judge(&machine("idle", Some(NOW - 9 * 60), true), None, NOW).want(),
            Want::Rest
        );
        assert_eq!(
            judge(&machine("idle", None, true), None, NOW).want(),
            Want::Needs
        );
    }

    #[test]
    fn behind_for_over_ten_minutes_needs_you() {
        let behind = judge(
            &machine("saved", Some(NOW - 60), false),
            Some(&record(NOW - 11 * 60)),
            NOW,
        );
        assert_eq!(behind.concerns, [Concern::Behind(NOW - 11 * 60)]);
        let fresh = judge(
            &machine("saved", Some(NOW - 60), false),
            Some(&record(NOW - 60)),
            NOW,
        );
        assert_eq!(fresh.want(), Want::Rest);
    }

    #[test]
    fn mneme_says_since_when_a_copy_is_behind() {
        let mut machine = machine("saved", Some(NOW - 60), false);
        machine.behind_since = Some(NOW - 5 * 60);
        let lately = judge(&machine, Some(&record(NOW - 3600)), NOW);
        assert_eq!(
            lately.want(),
            Want::Rest,
            "behind for five minutes, whatever the head's age"
        );
        machine.behind_since = Some(NOW - 11 * 60);
        let long = judge(&machine, Some(&record(NOW - 60)), NOW);
        assert_eq!(long.concerns, [Concern::Behind(NOW - 11 * 60)]);
    }

    #[test]
    fn offline_and_skipped_files_are_worth_a_look() {
        assert_eq!(
            judge(&machine("offline", Some(NOW - 60), true), None, NOW).want(),
            Want::Look
        );
        let mut skipping = machine("pushed", Some(NOW - 60), true);
        skipping.skipped = Some(vec![Skipped {
            path: "a.stl".into(),
            reason: "a file over 10 MiB".into(),
        }]);
        skipping.skipped_more = Some(2);
        let copy = judge(&skipping, None, NOW);
        assert_eq!((copy.want(), copy.left_out()), (Want::Look, 3));
    }

    #[test]
    fn named_machines_keep_their_places_and_others_take_what_is_left() {
        let copy = |name: &str| {
            judge(
                &Machine {
                    name: name.into(),
                    ..machine("idle", Some(NOW), true)
                },
                None,
                NOW,
            )
        };
        let copies = [copy("cassia"), copy("aeolus"), copy("dora")];
        let placed: Vec<_> = places(&copies)
            .into_iter()
            .map(|(p, c)| (p, c.map(|c| c.name.as_str())))
            .collect();
        assert_eq!(
            placed,
            [
                (Place::Blackbox, Some("cassia")),
                (Place::Aeolus, Some("aeolus"))
            ]
        );
    }
}
