//! Each wiki route in front of Mneme's example responses: its status and the facts on it.
//! Every check reads the HTML as served, so it holds with script off.

use std::time::Duration;

use atrium_wiki::stub::Stub;

use crate::html::{attrs, rows, texts};
use crate::support::Atrium;

const ALPHA: &str = "/wiki/page/4226c96dd1386f3a";
const HEAD: &str = "faf510a1fb457f1af5496ca64dea77ed68ddb2f8";

#[tokio::test]
async fn pages_are_listed_with_sort_and_order_links() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki").await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        texts(&reply.body, "tbody th"),
        [
            "Agents",
            "Alpha",
            "deep",
            "Deep again",
            "Gotländska minnen",
            "Home"
        ]
    );
    assert!(attrs(&reply.body, "tbody a", "href").contains(&ALPHA.to_owned()));
    assert_eq!(texts(&reply.body, "th[aria-sort=ascending]"), ["title"]);
    let links = attrs(&reply.body, "thead a", "href");
    assert!(links.contains(&"/wiki?sort=title&order=desc&offset=0".to_owned()));
    assert!(links.contains(&"/wiki?sort=updated&order=desc&offset=0".to_owned()));
    assert_eq!(
        atrium.mneme.requests(),
        ["/v1/pages?offset=0&limit=50&sort=title&order=asc"]
    );
}

#[tokio::test]
async fn home_shows_the_latest_changes_as_a_tile() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert_eq!(texts(&home.body, ".tile.changes h2"), ["Recent changes"]);
    assert_eq!(
        attrs(&home.body, ".tile.changes a.more", "href"),
        ["/wiki/changes"]
    );
    let changes = texts(&home.body, ".tile.changes ol li");
    assert_eq!(changes.len(), 8, "{changes:?}");
    assert!(
        changes[0].contains("greek/alpha.md edited"),
        "{}",
        changes[0]
    );
    assert_eq!(atrium.mneme.requests(), ["/v1/changes?offset=0&limit=8"]);
}

#[tokio::test]
async fn a_chosen_sort_reaches_mneme_and_is_marked() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki?sort=words&order=asc").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "th[aria-sort=ascending]"), ["words"]);
    assert_eq!(
        atrium.mneme.requests(),
        ["/v1/pages?offset=0&limit=50&sort=words&order=asc"]
    );
    assert_eq!(atrium.get("/wiki?sort=colour").await.status, 400);
}

#[tokio::test]
async fn a_page_shows_its_text_windows_and_recent_versions() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get(ALPHA).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["Alpha"]);
    assert_eq!(
        texts(&reply.body, "article p")[0],
        "The first page talks about boats and harbours."
    );
    assert_eq!(
        rows(&reply.body, "#activity + table tbody tr"),
        [
            ["the last 7 days", "0", "0", "0", "0"],
            ["the last 30 days", "0", "0", "0", "0"],
            ["the last 90 days", "3", "2", "18", "0"],
            ["all time", "3", "2", "18", "0"],
        ]
    );
    let recent = texts(&reply.body, "#recent + ol li");
    assert_eq!(recent.len(), 4);
    assert!(recent[1].contains("renamed from alpha.md"), "{}", recent[1]);
    let links = attrs(&reply.body, "#recent + ol a", "href");
    assert!(
        links.contains(&format!("{ALPHA}/diff?to={HEAD}")),
        "{links:?}"
    );
    assert!(links.contains(&format!("{ALPHA}/at/{HEAD}")), "{links:?}");
}

#[tokio::test]
async fn an_unknown_page_is_not_found() {
    let atrium = Atrium::tailnet().await;
    assert_eq!(atrium.get("/wiki/page/0123456789abcdef").await.status, 404);
    assert_eq!(atrium.get("/wiki/page/not-an-id").await.status, 404);
    assert_eq!(
        atrium.mneme.requests(),
        [
            "/v1/pages/0123456789abcdef",
            "/v1/pages/0123456789abcdef/versions?offset=0&limit=10"
        ],
        "an address that cannot be an id never reaches Mneme"
    );
}

#[tokio::test]
async fn the_history_lists_every_event() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get(&format!("{ALPHA}/history")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["History of Alpha"]);
    let events = texts(&reply.body, "main ol li");
    assert_eq!(events.len(), 4);
    assert!(
        events[0].starts_with("2026-09-01 01:40 edited by Writer, +3 −0 words."),
        "{}",
        events[0]
    );
    assert!(events[3].contains("created"), "{}", events[3]);
}

#[tokio::test]
async fn a_version_shows_the_text_at_that_commit() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get(&format!("{ALPHA}/at/{HEAD}")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["Alpha"]);
    assert_eq!(texts(&reply.body, "main > p code"), ["0735657"]);
    assert_eq!(
        texts(&reply.body, "article p"),
        ["The first page talks about boats and harbours."]
    );
    assert_eq!(atrium.get(&format!("{ALPHA}/at/abc")).await.status, 404);
}

#[tokio::test]
async fn a_diff_marks_added_and_removed_lines() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get(&format!("{ALPHA}/diff?to={HEAD}")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["Changes to Alpha"]);
    assert_eq!(
        texts(&reply.body, "ol.diff li"),
        [
            "3 unchanged lines",
            "",
            "Now with a second line.",
            "+",
            "+ And a third."
        ]
    );
    assert_eq!(texts(&reply.body, "ol.diff ins"), ["+", "+ And a third."]);
    assert_eq!(
        atrium.mneme.requests().last().unwrap(),
        &format!("/v1/pages/4226c96dd1386f3a/diff?to={HEAD}")
    );
    assert_eq!(
        atrium.get(&format!("{ALPHA}/diff?from=xyz")).await.status,
        400
    );
}

#[tokio::test]
async fn recent_changes_are_grouped_by_day() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki/changes").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "main h2"), ["2026-09-01"]);
    let events = texts(&reply.body, "main section li");
    assert_eq!(events.len(), 13);
    assert!(events[1].contains("AGENTS.md edited"), "{}", events[1]);
    assert!(events[1].contains("overlapping aeolus"), "{}", events[1]);
}

#[tokio::test]
async fn a_search_shows_snippets_with_the_hits_marked() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki/search?q=harbours").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "main h2"), ["1 page matches"]);
    assert_eq!(texts(&reply.body, "main ol li > a"), ["Alpha"]);
    assert_eq!(texts(&reply.body, "main ol li mark"), ["harbours"]);
    assert_eq!(attrs(&reply.body, "input[name=q]", "value"), ["harbours"]);
}

#[tokio::test]
async fn search_sort_and_filters_reach_mneme_from_a_plain_form() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium
        .get("/wiki/search?q=boats&sort=updated&order=asc&days=30")
        .await;
    assert_eq!(reply.status, 200);
    assert_eq!(attrs(&reply.body, "form", "method"), ["get"]);
    assert_eq!(
        attrs(&reply.body, "option[selected]", "value"),
        ["updated", "asc", "30"]
    );
    assert_eq!(
        atrium.mneme.requests(),
        ["/v1/search?offset=0&limit=50&sort=updated&order=asc&q=boats&changed_within_days=30"]
    );
}

#[tokio::test]
async fn an_empty_search_shows_the_form_and_asks_nothing() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki/search?q=&sort=&order=&days=").await;
    assert_eq!(reply.status, 200);
    assert_eq!(attrs(&reply.body, "input[name=q]", "value"), [""]);
    assert_eq!(atrium.mneme.requests(), Vec::<String>::new());
}

#[tokio::test]
async fn a_search_mneme_cannot_read_says_why() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki/search?q=%22open").await;
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("open quote"), "{}", reply.body);
}

#[tokio::test]
async fn machines_show_their_autosave_state() {
    let atrium = Atrium::tailnet().await;
    let reply = atrium.get("/wiki/machines").await;
    assert_eq!(reply.status, 200);
    let never = ["not reported", "no", "never", "never"];
    let at = "2026-11-11 09:38";
    assert_eq!(
        rows(&reply.body, "tbody tr"),
        [
            [&["aeolus"], &never[..]].concat(),
            vec![
                "blackbox",
                "blocked: a merge is in progress",
                "yes (faf510a)",
                at,
                at
            ],
            [&["cassia"], &never[..]].concat(),
        ]
    );
}

#[tokio::test]
async fn a_link_by_file_path_leads_to_the_page_id() {
    let atrium = Atrium::tailnet().await;
    let cases = [
        ("/wiki/greek/alpha.md", ALPHA),
        (
            "/wiki/minnen%20fr%C3%A5n%20gotland.md",
            "/wiki/page/27d4c6d2c38ddd0a",
        ),
    ];
    for (path, page) in cases {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, 303, "{path}");
        assert_eq!(reply.header("location"), Some(page), "{path}");
    }
    assert_eq!(atrium.get("/wiki/nothing.md").await.status, 404);
    assert_eq!(atrium.get("/wiki/greek").await.status, 404);
}

async fn away(stub: Stub) {
    let atrium = Atrium::start(stub, &["--viewer", "tailnet"]).await;
    for path in ["/wiki", ALPHA, "/wiki/changes", "/wiki/machines"] {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, 503, "{path}");
        assert_eq!(reply.header("retry-after"), Some("30"), "{path}");
        assert_eq!(texts(&reply.body, "h1"), ["The wiki is briefly away"]);
    }
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert!(
        texts(&home.body, ".tiles p")
            .iter()
            .any(|text| text.starts_with("The wiki is briefly away")),
        "{}",
        home.body
    );
}

#[tokio::test]
async fn a_mneme_that_is_down_is_away_and_home_still_works() {
    away(Stub::Down).await;
}

#[tokio::test]
async fn a_slow_mneme_is_away_after_the_timeout() {
    away(Stub::Slow(Duration::from_secs(5))).await;
}

#[tokio::test]
async fn a_failing_mneme_is_away() {
    away(Stub::Failing).await;
}
