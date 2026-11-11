//! The design's invented wiki, served by a stand-in for Mneme in this process, so the design
//! can be looked at, checked and captured without a Mneme. The content is the prototype's
//! `src/content.mjs`, in the shapes Mneme answers with, so every page goes through the same
//! client and templates as a real wiki.

use std::cmp::Reverse;
use std::fmt::Write;

use axum::Router;
use axum::extract::{Path, Query};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::stub::{self, Stub};

/// The demo's now: Wednesday 11 November 2026 at 09:41 in Stockholm.
pub const NOW: Timestamp = Timestamp::constant(1_794_386_460, 0);
/// The demo reader's last visit, at 07:48.
pub const LAST_VISIT: i64 = 1_794_379_680;
/// When the demo's record last answered before it went away, at 09:12.
pub const LAST_HEARD: i64 = 1_794_384_720;

/// Starts the stand-in and returns its base URL. With `away`, nothing answers there.
///
/// # Errors
///
/// Fails when no local port can be bound.
pub async fn serve(away: bool) -> Result<String, std::io::Error> {
    let running = stub::serve(if away { Stub::Down } else { Stub::Demo }).await;
    Ok(running.base)
}

/// A JSON answer, as Mneme gives it.
struct Json(Value);

impl IntoResponse for Json {
    fn into_response(self) -> Response {
        (
            [(header::CONTENT_TYPE, "application/json")],
            self.0.to_string(),
        )
            .into_response()
    }
}

/// A time in the demo, written as Stockholm's local time.
fn at(local: &str) -> i64 {
    local
        .parse::<DateTime>()
        .ok()
        .and_then(|time| time.to_zoned(TimeZone::get("Europe/Stockholm").ok()?).ok())
        .map_or(0, |zoned| zoned.timestamp().as_second())
}

/// A stable hexadecimal id of `length` characters for `seed`.
fn hex(seed: &str, length: usize) -> String {
    let mut out = String::with_capacity(length);
    let mut round: u64 = 0;
    while out.len() < length {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ round;
        for byte in seed.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        let _ = write!(out, "{hash:016x}");
        round += 1;
    }
    out.truncate(length);
    out
}

struct PageData {
    key: &'static str,
    title: &'static str,
    path: &'static str,
    words: u64,
    created: &'static str,
    updated: &'static str,
    versions: u64,
    churn: u64,
    text: &'static str,
}

const PAGES: [PageData; 16] = [
    PageData {
        key: "restic",
        title: "Restic backups on apollo",
        path: "homelab/backup-rotation.md",
        words: 1284,
        created: "2026-06-14T10:02",
        updated: "2026-11-11T08:12",
        versions: 23,
        churn: 4210,
        text: "apollo runs restic every night against two repositories. The first lives on the cluster's second pool and answers most restores in seconds. The offsite copy is on a borrowed disk, reached over the tailnet.",
    },
    PageData {
        key: "corne",
        title: "Keyboard build: Corne v4",
        path: "projects/corne-v4.md",
        words: 2210,
        created: "2026-09-02T19:40",
        updated: "2026-11-11T09:12",
        versions: 31,
        churn: 6120,
        text: "Choc switches on a split Corne. Wiring the thumb cluster, the switch tester results, firmware with home row mods, and a tenting stand cut from walnut offcuts.",
    },
    PageData {
        key: "autosave",
        title: "Autosave service",
        path: "meta/autosave.md",
        words: 860,
        created: "2026-05-20T08:15",
        updated: "2026-11-11T08:03",
        versions: 17,
        churn: 2390,
        text: "Each machine with a copy of the wiki folder runs the autosave service. It saves, pulls and pushes every two minutes, skips files over 20 MB, and stops when a pull would overwrite local edits.",
    },
    PageData {
        key: "acl",
        title: "Tailnet ACLs",
        path: "homelab/tailnet-acls.md",
        words: 540,
        created: "2026-04-11T21:30",
        updated: "2026-11-11T07:31",
        versions: 12,
        churn: 1104,
        text: "Only apollo may reach the backup target, on port 8000, and only between 03:00 and 06:00. Phones reach Atrium and nothing else.",
    },
    PageData {
        key: "plants",
        title: "Plant watering rota",
        path: "home/plants.md",
        words: 212,
        created: "2026-11-11T03:15",
        updated: "2026-11-11T03:15",
        versions: 1,
        churn: 212,
        text: "The fig wants water when the top two centimetres are dry, about every nine days in winter. The pothos forgives everything.",
    },
    PageData {
        key: "reading",
        title: "Reading list, winter 2026",
        path: "reading/winter-2026.md",
        words: 1530,
        created: "2026-10-01T20:12",
        updated: "2026-11-10T22:48",
        versions: 9,
        churn: 1880,
        text: "Eleven books for the dark months, sorted by how much attention each one asks for. Two essay collections for low days.",
    },
    PageData {
        key: "sourdough",
        title: "Sourdough schedule",
        path: "kitchen/sourdough.md",
        words: 430,
        created: "2026-03-08T09:00",
        updated: "2026-11-10T19:30",
        versions: 14,
        churn: 920,
        text: "Feed at 21:00, mix at 08:00, bulk until it has risen by half. Cold proof overnight when the kitchen is under 19 degrees.",
    },
    PageData {
        key: "bike",
        title: "Bike maintenance log",
        path: "home/bike.md",
        words: 690,
        created: "2026-02-16T18:22",
        updated: "2026-11-10T14:05",
        versions: 11,
        churn: 1012,
        text: "Chain measured at 0.5 percent in October. New brake pads front and rear before the winter roads. Tubeless sealant topped up every three months.",
    },
    PageData {
        key: "style",
        title: "Wiki style guide for agents",
        path: "meta/style-guide.md",
        words: 980,
        created: "2026-05-21T07:30",
        updated: "2026-11-09T05:40",
        versions: 19,
        churn: 3302,
        text: "Write short declarative sentences. Change a page only when something changed. Put questions for the owner under a heading named Open questions.",
    },
    PageData {
        key: "grinder",
        title: "Coffee grinder settings",
        path: "kitchen/grinder.md",
        words: 180,
        created: "2026-07-30T08:44",
        updated: "2026-11-09T00:20",
        versions: 6,
        churn: 240,
        text: "Step 14 for the V60, step 6 for the moka pot. Clean the burrs every second bag.",
    },
    PageData {
        key: "gpu",
        title: "blackbox GPU passthrough notes",
        path: "homelab/blackbox-gpu.md",
        words: 1120,
        created: "2026-08-19T22:10",
        updated: "2026-11-06T23:40",
        versions: 8,
        churn: 1740,
        text: "Snapshot the VM before changing the IOMMU groups. The nightly restic job skips the VM disk on purpose.",
    },
    PageData {
        key: "apollo",
        title: "apollo cluster layout",
        path: "homelab/apollo.md",
        words: 1460,
        created: "2026-01-04T15:00",
        updated: "2026-11-05T06:01",
        versions: 27,
        churn: 5210,
        text: "Three nodes behind one name. The wiki record lives on the replicated pool, so any one node may sleep without the record going away.",
    },
    PageData {
        key: "bread",
        title: "Rye starter notes",
        path: "kitchen/rye-starter.md",
        words: 320,
        created: "2026-03-09T09:20",
        updated: "2026-10-30T08:10",
        versions: 5,
        churn: 410,
        text: "Rye starter at 100 percent hydration. Smells of apples when it is ready, of nail varnish when it is hungry.",
    },
    PageData {
        key: "dns",
        title: "Home DNS and split horizon",
        path: "homelab/dns.md",
        words: 760,
        created: "2026-02-01T13:40",
        updated: "2026-10-28T20:15",
        versions: 10,
        churn: 1330,
        text: "Names under the home zone resolve to tailnet addresses from outside and to the house network from inside.",
    },
    PageData {
        key: "shelf",
        title: "Records to digitise",
        path: "home/records.md",
        words: 410,
        created: "2026-09-12T11:00",
        updated: "2026-10-21T17:32",
        versions: 4,
        churn: 520,
        text: "Forty records on the low shelf. Rip at 24 bit, keep the sleeve photos beside the files.",
    },
    PageData {
        key: "walks",
        title: "Walks within an hour",
        path: "home/walks.md",
        words: 880,
        created: "2026-04-27T16:05",
        updated: "2026-10-12T10:45",
        versions: 7,
        churn: 1190,
        text: "Seven walks that start from the door or a short train ride away, with where to sit and where the wind is worst.",
    },
];

/// The page that was deleted, which the record still remembers.
const GONE: PageData = PageData {
    key: "printer",
    title: "Old printer notes",
    path: "homelab/printer.md",
    words: 0,
    created: "2025-12-02T10:00",
    updated: "2026-11-08T11:02",
    versions: 6,
    churn: 960,
    text: "",
};

/// The pages the record counts, of which the demo holds the sixteen above.
const PAGE_COUNT: u64 = 64;
const COMMIT_COUNT: u64 = 3118;

/// One event: when, on which page, what, by whom, and how many words.
struct Change {
    at: &'static str,
    page: &'static str,
    kind: &'static str,
    author: &'static str,
    version: u64,
    words: u64,
    added: u64,
    removed: u64,
    from: Option<&'static str>,
    /// The page and version of the edit that lost the same lines.
    overlap: Option<(&'static str, u64)>,
}

const fn change(
    at: &'static str,
    page: &'static str,
    kind: &'static str,
    author: &'static str,
    version: u64,
    added: u64,
    removed: u64,
) -> Change {
    Change {
        at,
        page,
        kind,
        author,
        version,
        words: 0,
        added,
        removed,
        from: None,
        overlap: None,
    }
}

fn changes() -> Vec<Change> {
    vec![
        change(
            "2026-11-11T09:12",
            "corne",
            "edited",
            "blackbox",
            31,
            61,
            12,
        ),
        Change {
            overlap: Some(("restic", 21)),
            ..change(
                "2026-11-11T08:12",
                "restic",
                "edited",
                "blackbox",
                23,
                142,
                38,
            )
        },
        change(
            "2026-11-11T08:03",
            "autosave",
            "edited",
            "blackbox",
            17,
            18,
            40,
        ),
        change("2026-11-11T07:31", "acl", "edited", "aeolus", 12, 34, 29),
        change("2026-11-11T03:15", "plants", "created", "aeolus", 1, 212, 0),
        change("2026-11-10T22:48", "reading", "edited", "rowan", 9, 210, 0),
        change(
            "2026-11-10T19:30",
            "sourdough",
            "edited",
            "aeolus",
            14,
            12,
            9,
        ),
        Change {
            from: Some("home/bicycle.md"),
            ..change("2026-11-10T14:05", "bike", "renamed", "rowan", 11, 0, 0)
        },
        change(
            "2026-11-09T06:02",
            "restic",
            "edited",
            "blackbox",
            22,
            90,
            12,
        ),
        Change {
            overlap: Some(("style", 18)),
            ..change("2026-11-09T05:40", "style", "edited", "aeolus", 19, 55, 61)
        },
        change("2026-11-09T00:20", "grinder", "edited", "aeolus", 6, 8, 3),
        change(
            "2026-11-08T21:14",
            "corne",
            "edited",
            "blackbox",
            30,
            340,
            18,
        ),
        change("2026-11-08T11:02", "printer", "deleted", "rowan", 6, 0, 480),
        change("2026-11-07T20:41", "reading", "edited", "rowan", 8, 96, 40),
        change(
            "2026-11-07T04:12",
            "autosave",
            "edited",
            "aeolus",
            16,
            22,
            6,
        ),
    ]
}

/// Every event of the restic page, newest first, and one older for its 90 days.
fn restic_events() -> Vec<Change> {
    let edit = |at, author, version, words, added, removed| Change {
        words,
        ..change(at, "restic", "edited", author, version, added, removed)
    };
    vec![
        Change {
            overlap: Some(("restic", 21)),
            ..edit("2026-11-11T08:12", "blackbox", 23, 1284, 142, 38)
        },
        edit("2026-11-09T06:02", "blackbox", 22, 1180, 90, 12),
        edit("2026-11-05T06:01", "aeolus", 21, 1102, 41, 23),
        edit("2026-10-28T06:04", "rowan", 20, 1084, 64, 0),
        edit("2026-10-26T06:00", "blackbox", 19, 1020, 120, 44),
        Change {
            overlap: Some(("restic", 17)),
            ..edit("2026-10-19T22:31", "aeolus", 18, 944, 31, 30)
        },
        edit("2026-10-12T06:02", "blackbox", 17, 943, 208, 96),
        edit("2026-09-30T06:01", "blackbox", 16, 831, 55, 12),
        Change {
            from: Some("homelab/backups.md"),
            words: 788,
            ..change("2026-09-21T19:48", "restic", "renamed", "rowan", 15, 0, 0)
        },
        edit("2026-09-14T06:03", "aeolus", 14, 788, 77, 20),
        edit("2026-08-20T06:02", "blackbox", 13, 711, 96, 30),
    ]
}

/// The older edits other pages' overlaps point at, which only those pages' histories list.
fn lost_edits() -> Vec<Change> {
    vec![change(
        "2026-11-09T05:38",
        "style",
        "edited",
        "blackbox",
        18,
        12,
        4,
    )]
}

fn page_of(key: &str) -> Option<&'static PageData> {
    PAGES
        .iter()
        .chain(std::iter::once(&GONE))
        .find(|page| page.key == key)
}

fn page_id(key: &str) -> String {
    hex(&format!("page {key}"), 16)
}

fn commit(page: &str, version: u64) -> String {
    hex(&format!("commit {page} {version}"), 40)
}

fn page_json(page: &PageData) -> Value {
    let gone = page.key == GONE.key;
    json!({
        "id": page_id(page.key),
        "path": page.path,
        "title": page.title,
        "words": page.words,
        "created": at(page.created),
        "updated": at(page.updated),
        "versions": page.versions,
        "churn": page.churn,
        "deleted": gone.then(|| at(GONE.updated)),
    })
}

fn event_json(change: &Change) -> Value {
    let page = page_of(change.page);
    let path = page.map_or("", |page| page.path);
    let words = if change.words > 0 {
        change.words
    } else {
        page.map_or(0, |page| page.words)
    };
    json!({
        "commit": commit(change.page, change.version),
        "page": page_id(change.page),
        "path": path,
        "time": at(change.at),
        "author": change.author,
        "kind": change.kind,
        "from": change.from,
        "version": change.kind != "renamed",
        "words": if change.kind == "deleted" { 0 } else { words },
        "added": change.added,
        "removed": change.removed,
        "overlap": change.overlap.map(|(page, version)| commit(page, version)),
        "overlap_author": change.overlap.and_then(|(page, version)| {
            changes()
                .into_iter()
                .chain(restic_events())
                .find(|lost| lost.page == page && lost.version == version)
                .map(|lost| lost.author)
        }),
    })
}

const RESTIC: &str = r"# Restic backups on apollo

## Summary

apollo runs restic every night against two repositories. The first lives on the cluster's replicated pool, `tank`, and answers most restores in seconds. The second is offsite, on a borrowed disk at a friend's flat, reached over the tailnet through the rules in [Tailnet ACLs](tailnet-acls.md).

Nothing here needs a person on a normal night. Whoever writes this page reads the job logs at 06:00 and changes a line only when something changed, so a quiet page means a quiet week. The wiki folder itself is saved by the [autosave service](../meta/autosave.md), not by these jobs.

## Schedule

| Job | When | Keeps | Target |
| --- | --- | --- | --- |
| home | 03:00 nightly | 7 daily, 4 weekly, 12 monthly | `tank/restic` |
| photos | 03:40 nightly | 30 daily, 12 monthly | offsite |
| wiki | hourly | 48 hourly, 30 daily | `tank/restic` |
| blackbox | 04:20 nightly | 7 daily, 8 weekly | `tank/restic` |

## Restore a file

Find the snapshot first, then restore into a scratch folder and copy back by hand. Restoring in place has bitten once already; see [known issues](#known-issues).

```sh
restic -r /tank/restic snapshots --tag home --latest 3
restic -r /tank/restic restore latest \
  --target /tmp/restore \
  --include /home/notes/garden.md
diff /tmp/restore/home/notes/garden.md ~/notes/garden.md
```

## Restore drill, November

Once a month three files are picked at random and restored from each repository. This month:

- A 1.2 GB photo folder came back from offsite in 14 minutes, at about 1.4 MB/s.
- The wiki's hourly snapshot from 02:00 matched the record byte for byte.
- A notes file from September restored from `tank` in under a second.
- The `check --read-data-subset=5%` pass found no damaged packs.

## Known issues

1. The offsite job stalls when a node of [apollo](apollo.md) sleeps mid-run. It resumes on its own, but the morning summary reports it as failed.
2. Restoring in place over a file that the sync client holds open leaves a zero-byte file. Restore to `/tmp` first.

## Open questions

- Should the offsite copy run weekly instead of nightly while the borrowed disk is this slow?

## Related

The cluster itself is in [apollo cluster layout](apollo.md), who may reach the backup target is in [Tailnet ACLs](tailnet-acls.md), and the workstation's own exceptions are in [blackbox GPU passthrough notes](blackbox-gpu.md).
";

const RESTIC_V20: &str = r"# Restic backups on apollo

## Summary

apollo runs restic every night against two repositories. The first lives on the cluster's replicated pool, `tank`. The second is offsite, on a borrowed disk at a friend's flat.

The job logs are read at 07:00 every morning and a line is written here every day, even when nothing changed.

## Schedule

| Job | When | Keeps |
| --- | --- | --- |
| home | 03:00 nightly | 7 daily, 4 weekly |
| photos | 03:40 nightly | 30 daily |
| wiki | daily | 30 daily |

## Restore a file

Restore into a scratch folder and copy back by hand.

## Known issues

1. The offsite job needs a manual restart when a node of apollo sleeps mid-run.
";

/// The text of a page that only has a summary.
fn markdown(page: &PageData) -> String {
    match page.key {
        "restic" => RESTIC.to_owned(),
        "autosave" => format!(
            "# {}\n\n{}\n\n## Open questions\n\n- Is 20 MB still the right limit now that STL files live in scratch/?\n",
            page.title, page.text
        ),
        "corne" => format!(
            "# {}\n\n{}\n\n## Open questions\n\n- Keep the 200 ms tapping term on the pinkies, or try 180?\n",
            page.title, page.text
        ),
        _ => format!("# {}\n\n{}\n", page.title, page.text),
    }
}

/// The paging and filters Mneme reads, as far as the demo uses them.
#[derive(Debug, Deserialize)]
struct Params {
    offset: Option<usize>,
    limit: Option<usize>,
    sort: Option<String>,
    order: Option<String>,
    deleted: Option<bool>,
    since: Option<i64>,
    before: Option<i64>,
    q: Option<String>,
}

impl Params {
    fn cut<T>(&self, items: Vec<T>) -> Vec<T> {
        items
            .into_iter()
            .skip(self.offset.unwrap_or(0))
            .take(self.limit.unwrap_or(50).min(500))
            .collect()
    }
}

fn sorted(params: &Params, mut pages: Vec<&'static PageData>) -> Vec<&'static PageData> {
    let sort = params.sort.as_deref().unwrap_or("title");
    match sort {
        "updated" => pages.sort_by_key(|page| Reverse(at(page.updated))),
        "created" => pages.sort_by_key(|page| Reverse(at(page.created))),
        "versions" => pages.sort_by_key(|page| Reverse(page.versions)),
        "words" => pages.sort_by_key(|page| Reverse(page.words)),
        "churn" => pages.sort_by_key(|page| Reverse(page.churn)),
        _ => pages.sort_by_key(|page| page.title.to_lowercase()),
    }
    let reverse = match (sort, params.order.as_deref()) {
        ("title", Some("desc")) => true,
        ("title", _) | (_, Some("desc") | None) => false,
        (_, Some(_)) => true,
    };
    if reverse {
        pages.reverse();
    }
    pages
}

async fn pages(Query(params): Query<Params>) -> Response {
    let mut all: Vec<&PageData> = PAGES.iter().collect();
    if params.deleted == Some(true) {
        all.push(&GONE);
    }
    let all = sorted(&params, all);
    let shown: Vec<Value> = params.cut(all).into_iter().map(page_json).collect();
    Json(json!({ "total": PAGE_COUNT, "pages": shown })).into_response()
}

fn by_id(id: &str) -> Option<&'static PageData> {
    PAGES
        .iter()
        .chain(std::iter::once(&GONE))
        .find(|page| page_id(page.key) == id)
}

fn missing() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "no such page" })),
    )
        .into_response()
}

async fn page(Path(id): Path<String>) -> Response {
    let Some(page) = by_id(&id) else {
        return missing();
    };
    let gone = page.key == GONE.key;
    let windows = if page.key == "restic" {
        json!([
            { "days": 7, "versions": 3, "sessions": 3, "added": 273, "removed": 73 },
            { "days": 30, "versions": 6, "sessions": 5, "added": 488, "removed": 147 },
            { "days": 90, "versions": 11, "sessions": 9, "added": 924, "removed": 305 },
            { "days": null, "versions": 23, "sessions": 18, "added": 3247, "removed": 1963 },
        ])
    } else {
        let window = |days: Value| json!({ "days": days, "versions": 1, "sessions": 1, "added": 0, "removed": 0 });
        json!([
            window(json!(7)),
            window(json!(30)),
            window(json!(90)),
            window(Value::Null)
        ])
    };
    Json(json!({
        "page": page_json(page),
        "markdown": (!gone).then(|| markdown(page)),
        "windows": windows,
    }))
    .into_response()
}

fn events_of(key: &str) -> Vec<Change> {
    if key == "restic" {
        return restic_events();
    }
    let mut events: Vec<Change> = changes()
        .into_iter()
        .chain(lost_edits())
        .filter(|change| change.page == key)
        .collect();
    events.sort_by_key(|change| Reverse(at(change.at)));
    events
}

fn listed(params: &Params, events: Vec<Change>, total: Option<usize>) -> Response {
    let events: Vec<Change> = events
        .into_iter()
        .filter(|change| params.since.is_none_or(|since| at(change.at) >= since))
        .filter(|change| params.before.is_none_or(|before| at(change.at) < before))
        .collect();
    let total = total.unwrap_or(events.len());
    let shown: Vec<Value> = params.cut(events).iter().map(event_json).collect();
    Json(json!({ "total": total, "events": shown })).into_response()
}

async fn versions(Path(id): Path<String>, Query(params): Query<Params>) -> Response {
    let Some(page) = by_id(&id) else {
        return missing();
    };
    let events = events_of(page.key);
    let total = (page.key == "restic" && params.since.is_none()).then_some(23);
    listed(&params, events, total)
}

async fn version(Path((id, asked)): Path<(String, String)>) -> Response {
    let Some(page) = by_id(&id) else {
        return missing();
    };
    let text = if page.key == "restic" && asked == commit("restic", 20) {
        RESTIC_V20.to_owned()
    } else {
        markdown(page)
    };
    Json(json!({ "page": id, "commit": asked, "markdown": text })).into_response()
}

fn spans(parts: &[(&str, bool)]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|(text, changed)| json!({ "text": text, "changed": changed }))
            .collect(),
    )
}

async fn diff(Path(id): Path<String>) -> Response {
    let Some(page) = by_id(&id) else {
        return missing();
    };
    if page.key != "restic" {
        return Json(json!({ "from": null, "to": commit(page.key, page.versions), "lines": [] }))
            .into_response();
    }
    let same = |text: &str| json!({ "kind": "same", "value": text });
    let removed = |parts: &[(&str, bool)]| json!({ "kind": "removed", "value": spans(parts) });
    let added = |parts: &[(&str, bool)]| json!({ "kind": "added", "value": spans(parts) });
    let lines = json!([
        { "kind": "skipped", "value": 4 },
        same("apollo runs restic every night against two repositories. The first lives on the cluster's replicated pool, `tank`, and answers most restores in seconds."),
        same(""),
        removed(&[("Nothing here needs a person on a normal night. Whoever writes this page reads the job logs at ", false), ("07:00", true), (" and ", false), ("writes a line every morning", true), (".", false)]),
        added(&[("Nothing here needs a person on a normal night. Whoever writes this page reads the job logs at ", false), ("06:00", true), (" and ", false), ("changes a line only when something changed, so a quiet page means a quiet week", true), (".", false)]),
        same(""),
        { "kind": "skipped", "value": 14 },
        same("Find the snapshot first, then restore into a scratch folder and copy back by hand."),
        same(""),
        added(&[("Once a month three files are picked at random and restored from each repository. This month a 1.2 GB photo folder came back from offsite in 14 minutes, the wiki's hourly snapshot matched the record byte for byte, and the check pass found no damaged packs.", true)]),
        added(&[]),
        { "kind": "skipped", "value": 9 },
        same("Restoring in place over a file that the sync client holds open leaves a zero-byte file."),
        same(""),
        removed(&[("The offsite job stalls when a node of apollo sleeps mid-run. It ", false), ("needs a manual restart", true), (".", false)]),
        added(&[("The offsite job stalls when a node of apollo sleeps mid-run. It ", false), ("resumes on its own, but the morning summary reports it as failed", true), (".", false)]),
    ]);
    Json(json!({ "from": commit("restic", 22), "to": commit("restic", 23), "lines": lines }))
        .into_response()
}

async fn all_changes(Query(params): Query<Params>) -> Response {
    listed(&params, changes(), None)
}

/// Pages whose title or text holds the words, as one phrase, with the matches marked.
async fn search(Query(params): Query<Params>) -> Response {
    let words = params.q.clone().unwrap_or_default().to_lowercase();
    let hits: Vec<Value> = PAGES
        .iter()
        .filter(|page| {
            !words.is_empty()
                && format!("{} {}", page.title, page.text)
                    .to_lowercase()
                    .contains(&words)
        })
        .map(|page| {
            // Mneme cuts its own snippets; the demo cuts as the prototype did: from the
            // sentence that holds the first match, 200 characters, then an ellipsis.
            let whole = page.text;
            let first = whole.to_lowercase().find(&words).unwrap_or(0);
            let start = whole[..first].rfind(". ").map_or(0, |dot| dot + 2);
            let end = whole[start..]
                .char_indices()
                .nth(200)
                .map_or(whole.len(), |(i, _)| start + i);
            let text = &whole[start..end];
            let lower = text.to_lowercase();
            let mut runs = Vec::new();
            let mut rest = 0;
            for (start, _) in lower.match_indices(&words) {
                if start > rest {
                    runs.push(json!({ "text": &text[rest..start], "hit": false }));
                }
                runs.push(json!({ "text": &text[start..start + words.len()], "hit": true }));
                rest = start + words.len();
            }
            if rest < text.len() {
                runs.push(json!({ "text": &text[rest..], "hit": false }));
            }
            if end < whole.len() {
                runs.push(json!({ "text": "…", "hit": false }));
            }
            json!({ "page": page_json(page), "score": 1.0, "snippet": runs })
        })
        .collect();
    let total = hits.len();
    Json(json!({ "total": total, "hits": params.cut(hits) })).into_response()
}

async fn status() -> Response {
    Json(json!({
        "head": commit("corne", 31),
        "head_time": at("2026-11-11T09:12"),
        "pages": PAGE_COUNT,
        "commits": COMMIT_COUNT,
        "build_ms": 40,
    }))
    .into_response()
}

async fn machines() -> Response {
    Json(json!({
        "total": 2,
        "machines": [
            {
                "name": "blackbox",
                "seen": at("2026-11-11T09:40"),
                "reported": at("2026-11-11T09:40"),
                "state": "pushed",
                "detail": "",
                "head": commit("corne", 31),
                "current": true,
                "skipped": [{ "path": "scratch/corne-case.stl", "reason": "a file over 10 MiB" }],
                "skipped_more": 0,
            },
            {
                "name": "aeolus",
                "seen": at("2026-11-11T09:38"),
                "reported": at("2026-11-11T09:38"),
                "state": "blocked",
                "detail": "A pull would overwrite its local edits.",
                "head": commit("acl", 12),
                "current": false,
                "skipped": [],
                "skipped_more": 0,
            },
        ],
    }))
    .into_response()
}

async fn questions() -> Response {
    let question = |key: &str, text: &str| {
        let page = page_of(key).map_or(&GONE, |page| page);
        json!({
            "page": { "id": page_id(page.key), "path": page.path, "title": page.title },
            "text": text,
            "updated": at(page.updated),
        })
    };
    Json(json!({
        "total": 3,
        "questions": [
            question("restic", "Should the offsite copy run weekly instead of nightly while the borrowed disk is this slow?"),
            question("autosave", "Is 20 MB still the right limit now that STL files live in scratch/?"),
            question("corne", "Keep the 200 ms tapping term on the pinkies, or try 180?"),
        ],
    }))
    .into_response()
}

/// Every route the demo's stand-in answers.
pub fn routes() -> Router {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/pages", get(pages))
        .route("/v1/pages/{id}", get(page))
        .route("/v1/pages/{id}/versions", get(versions))
        .route("/v1/pages/{id}/versions/{commit}", get(version))
        .route("/v1/pages/{id}/diff", get(diff))
        .route("/v1/changes", get(all_changes))
        .route("/v1/search", get(search))
        .route("/v1/machines", get(machines))
        .route("/v1/questions", get(questions))
        .route("/v1/paths/{*path}", get(path))
}

/// The page at a path now, or the one most recently renamed away from it, as Mneme finds
/// it: never a deleted page.
async fn path(Path(asked): Path<String>) -> Response {
    let found = |page: &PageData, current: bool| {
        Json(json!({ "id": page_id(page.key), "path": page.path, "current": current }))
            .into_response()
    };
    if let Some(page) = PAGES.iter().find(|page| page.path == asked) {
        return found(page, true);
    }
    changes()
        .into_iter()
        .filter(|change| change.from == Some(asked.as_str()))
        .find_map(|change| page_of(change.page).filter(|page| page.key != GONE.key))
        .map_or_else(missing, |page| found(page, false))
}

/// The demo's page ids and commits, which the parity check and tests address pages by.
#[must_use]
pub fn address(key: &str) -> String {
    page_id(key)
}

/// The commit of version `version` of the page `key`.
#[must_use]
pub fn commit_of(key: &str, version: u64) -> String {
    commit(key, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_stockholms() {
        assert_eq!(at("2026-11-11T09:41"), NOW.as_second());
        assert_eq!(at("2026-11-11T07:48"), LAST_VISIT);
        assert_eq!(at("2026-11-11T09:12"), LAST_HEARD);
    }

    #[test]
    fn the_parity_check_addresses_the_demos_pages() {
        let routes = include_str!("../../../tools/check/routes.mjs");
        assert!(routes.contains(&format!("'/wiki/page/{}'", address("restic"))));
        for version in [20, 22, 23] {
            assert!(
                routes.contains(&format!("'{}'", commit_of("restic", version))),
                "v{version}"
            );
        }
    }

    #[test]
    fn ids_have_mnemes_shapes() {
        assert!(address("restic").parse::<crate::mneme::PageId>().is_ok());
        assert!(
            commit_of("restic", 23)
                .parse::<crate::mneme::CommitId>()
                .is_ok()
        );
        assert_ne!(address("restic"), address("corne"));
    }
}
