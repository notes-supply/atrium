//! Each wiki route in front of Mneme's example responses: its status and the facts on it.
//! Every check reads the HTML as served, so it holds with script off.

use std::time::Duration;

use atrium_wiki::stub::Stub;

use crate::html::{attrs, rows, texts};
use crate::support::Atrium;

const ALPHA: &str = "/wiki/page/4226c96dd1386f3a";
const HEAD: &str = "faf510a1fb457f1af5496ca64dea77ed68ddb2f8";

#[tokio::test]
async fn pages_are_listed_newest_first_with_sort_and_order_links() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["The wiki"]);
    let titles = texts(&reply.body, "tbody a.ttl");
    assert_eq!(titles.len(), 6, "{titles:?}");
    assert!(attrs(&reply.body, "tbody a.ttl", "href").contains(&ALPHA.to_owned()));
    assert_eq!(texts(&reply.body, "th[aria-sort=descending]"), ["Updated"]);
    let links = attrs(&reply.body, "thead a", "href");
    assert!(
        links.contains(&"/wiki?sort=updated&order=asc".to_owned()),
        "{links:?}"
    );
    assert!(
        links.contains(&"/wiki?sort=title&order=asc".to_owned()),
        "{links:?}"
    );
    assert_eq!(
        texts(&reply.body, ".order a[aria-current]"),
        ["Newest first"]
    );
    assert!(
        atrium
            .mneme
            .requests()
            .contains(&"/v1/pages?offset=0&limit=16&sort=updated&order=desc".to_owned())
    );
}

#[tokio::test]
async fn a_chosen_sort_reaches_mneme_and_is_marked() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki?sort=words&order=asc").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "th[aria-sort=ascending]"), ["Words"]);
    assert!(
        atrium
            .mneme
            .requests()
            .contains(&"/v1/pages?offset=0&limit=16&sort=words&order=asc".to_owned())
    );
    assert_eq!(atrium.get("/wiki?sort=colour").await.status, 400);
}

#[tokio::test]
async fn home_opens_with_the_sentence_the_map_and_the_tiles() {
    let atrium = Atrium::owner().await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    let sentence = texts(&home.body, "p.sentence")[0].clone();
    assert!(sentence.contains("ironbark is blocked"), "{sentence}");
    assert!(
        sentence.contains("Two questions are waiting for you."),
        "{sentence}"
    );
    assert_eq!(texts(&home.body, ".tile.changes h2"), ["Recent changes"]);
    assert_eq!(
        attrs(&home.body, ".tile.changes a.more", "href"),
        ["/wiki/changes"]
    );
    assert_eq!(texts(&home.body, ".tile.changes ol li").len(), 8);
    assert_eq!(texts(&home.body, "#waiting .q-text").len(), 2);
    assert_eq!(
        attrs(&home.body, ".tile.resume", "hidden"),
        [""],
        "filled by the browser"
    );
    assert_eq!(
        attrs(&home.body, ".map-svg .m", "data-m"),
        ["laurel", "ironbark", "willow"]
    );
    assert_eq!(
        attrs(&home.body, ".dock-card", "data-want"),
        ["2", "0", "2"],
        "ironbark is blocked, willow has not been seen"
    );
}

#[tokio::test]
async fn a_page_shows_its_text_windows_and_recent_versions() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get(ALPHA).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["Alpha"]);
    assert_eq!(
        texts(&reply.body, "article.paper > p")[0],
        "The first page talks about boats and harbours."
    );
    assert_eq!(
        rows(&reply.body, "table.windows tbody tr"),
        [
            ["7 days", "0", "0", "0", "0"],
            ["30 days", "0", "0", "0", "0"],
            ["90 days", "3", "2", "18", "0"],
            ["All time", "3", "2", "18", "0"],
        ]
    );
    let recent = texts(&reply.body, "ol.versions li");
    assert_eq!(recent.len(), 4);
    assert!(recent[0].starts_with("v4"), "{}", recent[0]);
    let links = attrs(&reply.body, "ol.versions a", "href");
    assert_eq!(links[0], ALPHA, "the newest version is the page");
    assert!(links[1].starts_with(&format!("{ALPHA}/at/")), "{links:?}");
}

#[tokio::test]
async fn an_unknown_page_is_not_found() {
    let atrium = Atrium::owner().await;
    assert_eq!(atrium.get("/wiki/page/0123456789abcdef").await.status, 404);
    assert_eq!(atrium.get("/wiki/page/not-an-id").await.status, 404);
    assert!(
        atrium
            .mneme
            .requests()
            .iter()
            .all(|request| !request.contains("not-an-id")),
        "an address that cannot be an id never reaches Mneme"
    );
}

#[tokio::test]
async fn the_history_lists_every_event() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get(&format!("{ALPHA}/history")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["History of Alpha"]);
    let events = rows(&reply.body, "tbody tr");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0][0], "v4");
    assert_eq!(events[0][2], "edited");
    assert_eq!(
        events[1][2], "renamedfrom alpha",
        "the old path sits below the kind"
    );
    assert_eq!(events[3][2], "created");
}

#[tokio::test]
async fn a_version_shows_the_text_at_that_commit() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get(&format!("{ALPHA}/at/{HEAD}")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["Alpha"]);
    assert_eq!(
        texts(&reply.body, ".old-band b"),
        ["version 1 of 4"],
        "the text at HEAD was left by the first commit"
    );
    assert_eq!(
        texts(&reply.body, "article.paper > p"),
        ["The first page talks about boats and harbours."]
    );
    assert_eq!(atrium.get(&format!("{ALPHA}/at/abc")).await.status, 404);
}

#[tokio::test]
async fn a_diff_marks_added_and_removed_words() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get(&format!("{ALPHA}/diff?to={HEAD}")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h1"), ["What changed"]);
    assert_eq!(
        texts(&reply.body, ".diff-body p.same"),
        ["Now with a second line."]
    );
    assert_eq!(texts(&reply.body, ".diff-body ins"), ["And a third."]);
    assert_eq!(texts(&reply.body, ".line-diff li.add").len(), 2);
    assert!(
        atrium
            .mneme
            .requests()
            .contains(&format!("/v1/pages/4226c96dd1386f3a/diff?to={HEAD}"))
    );
    let only = atrium
        .get(&format!("{ALPHA}/diff?to={HEAD}&only=changed"))
        .await;
    assert_eq!(texts(&only.body, ".diff-body p.same"), Vec::<String>::new());
    assert_eq!(
        atrium.get(&format!("{ALPHA}/diff?from=xyz")).await.status,
        400
    );
}

#[tokio::test]
async fn recent_changes_are_grouped_by_day_with_their_overlaps() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/changes").await;
    assert_eq!(reply.status, 200);
    assert_eq!(texts(&reply.body, "h2.day").len(), 1);
    let events = texts(&reply.body, ".days li");
    assert_eq!(events.len(), 13);
    assert!(
        events[1].contains("Writer edited the same lines"),
        "{}",
        events[1]
    );
    assert_eq!(
        attrs(&reply.body, ".days li .ov a", "href"),
        ["/wiki/page/9441e2ad011c494e/diff?to=0735657cc907aa6c418fac6c1f1277edbe547ec8"],
        "the author Mneme names, without reading the page's history"
    );
    assert!(
        !atrium
            .mneme
            .requests()
            .iter()
            .any(|request| request.starts_with("/v1/pages/9441e2ad011c494e/versions"))
    );
}

#[tokio::test]
async fn a_search_shows_snippets_with_the_hits_marked() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/search?q=harbours").await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        texts(&reply.body, "#find-count"),
        ["1 page matches harbours, by relevance"]
    );
    assert_eq!(texts(&reply.body, ".result-list h3"), ["Alpha"]);
    assert_eq!(texts(&reply.body, ".result-list mark"), ["harbours"]);
    assert_eq!(attrs(&reply.body, "input[name=q]", "value"), ["harbours"]);
}

#[tokio::test]
async fn search_sort_and_filters_reach_mneme_from_a_plain_form() {
    let atrium = Atrium::owner().await;
    let reply = atrium
        .get("/wiki/search?q=boats&sort=updated&changed=30&vmin=2")
        .await;
    assert_eq!(reply.status, 200);
    assert_eq!(attrs(&reply.body, "form.find-form", "method"), ["get"]);
    assert_eq!(
        attrs(&reply.body, "option[selected]", "value"),
        ["30", "", "updated"]
    );
    assert_eq!(texts(&reply.body, ".filters summary b"), ["two set"]);
    assert!(atrium.mneme.requests().contains(
        &"/v1/search?offset=0&limit=50&sort=updated&changed_within_days=30&min_versions=2&q=boats"
            .to_owned()
    ));
}

#[tokio::test]
async fn an_empty_search_shows_the_form_and_searches_nothing() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/search?q=&sort=&changed=").await;
    assert_eq!(reply.status, 200);
    assert_eq!(attrs(&reply.body, "input[name=q]", "value"), [""]);
    assert_eq!(
        attrs(&reply.body, ".results .search-state", "data-state"),
        ["empty"]
    );
    assert!(
        atrium
            .mneme
            .requests()
            .iter()
            .all(|r| !r.starts_with("/v1/search"))
    );
}

#[tokio::test]
async fn a_search_mneme_cannot_read_says_why() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/search?q=%22open").await;
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("open quote"), "{}", reply.body);
}

#[tokio::test]
async fn machines_show_their_state_and_what_they_left_out() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/machines").await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        texts(&reply.body, "section.mach h2"),
        ["willow", "ironbark", "cassia", "laurel"]
    );
    assert_eq!(
        attrs(&reply.body, "section.mach", "data-want"),
        ["2", "2", "2", "0"]
    );
    assert_eq!(
        texts(&reply.body, "#ironbark .why"),
        ["a merge is in progress"]
    );
    assert_eq!(
        texts(&reply.body, "#ironbark .skips code"),
        ["notes/scan.pdf"]
    );
    assert_eq!(
        texts(&reply.body, "#ironbark .skips span"),
        ["A file over 10 MiB."]
    );
    assert_eq!(texts(&reply.body, "#willow .word"), ["not reported"]);
}

#[tokio::test]
async fn a_link_by_file_path_leads_to_the_page_id() {
    let atrium = Atrium::owner().await;
    let cases = [
        ("/wiki/greek/alpha.md", ALPHA),
        (
            "/wiki/minnen%20fr%C3%A5n%20gotland.md",
            "/wiki/page/27d4c6d2c38ddd0a",
        ),
        ("/wiki/alpha.md", ALPHA),
    ];
    for (path, page) in cases {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, 303, "{path}");
        assert_eq!(reply.header("location"), Some(page), "{path}");
    }
    assert_eq!(atrium.get("/wiki/nothing.md").await.status, 404);
    assert_eq!(atrium.get("/wiki/greek").await.status, 404);
    let asked = atrium.mneme.requests();
    assert!(
        asked.contains(&"/v1/paths/minnen%20fr%C3%A5n%20gotland.md".to_owned()),
        "{asked:?}"
    );
    assert!(
        !asked
            .iter()
            .any(|request| request.starts_with("/v1/changes")),
        "no walk over the changes: {asked:?}"
    );
}

#[tokio::test]
async fn older_changes_are_the_days_before_the_time_the_address_names() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    // Monday 9 November 2026 at midnight in Stockholm. The offset is what a Mneme without
    // `before` would page by, and would show Tuesday again.
    let reply = atrium
        .get("/wiki/changes?before=1794178800&offset=3")
        .await
        .body;
    assert_eq!(
        attrs(&reply, "h2.day time", "datetime"),
        ["2026-11-08T21:14:00+01:00", "2026-11-07T20:41:00+01:00"]
    );
    assert_eq!(
        attrs(&reply, ".pager a[rel=prev]", "href"),
        ["/wiki/changes"]
    );
}

#[tokio::test]
async fn a_mneme_that_does_not_page_by_time_pages_by_offset() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/changes?before=1788225000&offset=5").await;
    assert_eq!(reply.status, 200);
    let asked = atrium.mneme.requests();
    assert!(
        asked.contains(&"/v1/changes?offset=0&limit=500&before=1788225000".to_owned()),
        "{asked:?}"
    );
    assert!(
        asked.contains(&"/v1/changes?offset=5&limit=500".to_owned()),
        "{asked:?}"
    );
}

#[tokio::test]
async fn a_path_held_now_by_a_new_page_leads_to_it_not_the_deleted_one() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/notes/deep.md").await;
    assert_eq!(reply.status, 303);
    assert_eq!(
        reply.header("location"),
        Some("/wiki/page/eaff4eab5f29fcc2")
    );
}

#[tokio::test]
async fn an_old_path_still_lands_after_a_rename_or_a_deletion() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    let renamed = atrium.get("/wiki/home/bicycle.md").await;
    assert_eq!(renamed.status, 303);
    let bike = renamed.header("location").unwrap().to_owned();
    assert_eq!(
        texts(&atrium.get(&bike).await.body, "h1"),
        ["Bike maintenance log"]
    );
    let gone = atrium.get("/wiki/homelab/printer.md").await;
    assert_eq!(gone.status, 404);
    assert_eq!(texts(&gone.body, ".gone .ttl-gone"), ["Old printer notes"]);
    assert!(texts(&gone.body, ".gone .diff-sum")[0].ends_with("by rowan"));
}

async fn away(stub: Stub) {
    let atrium = Atrium::start(stub, &["--viewer", "owner"]).await;
    for path in ["/wiki", ALPHA, "/wiki/changes", "/wiki/machines"] {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, 503, "{path}");
        assert_eq!(reply.header("retry-after"), Some("30"), "{path}");
        let sentence = texts(&reply.body, "p.sentence")[0].clone();
        assert!(
            sentence.contains("The wiki is out of reach just now."),
            "{sentence}"
        );
        assert_eq!(attrs(&reply.body, ".away-note a.btn", "href"), [path]);
    }
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert!(
        texts(&home.body, ".tiles p")
            .iter()
            .any(|text| text.contains("briefly away") || text.contains("Your place in each page")),
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

#[tokio::test]
async fn home_does_not_wait_for_a_slow_mneme() {
    let atrium = Atrium::start(Stub::Slow(Duration::from_secs(5)), &["--viewer", "owner"]).await;
    let start = std::time::Instant::now();
    let home = atrium.get("/").await;
    let took = start.elapsed();
    assert_eq!(home.status, 200);
    assert!(took < Duration::from_millis(900), "home took {took:?}");
    assert!(
        home.body.contains("The wiki is out of reach just now."),
        "{}",
        home.body
    );
}

#[tokio::test]
async fn the_modules_read_the_page_index_and_previews_as_json() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    let index = atrium.get("/wiki/index.json").await;
    assert_eq!(index.status, 200);
    assert_eq!(index.header("content-type"), Some("application/json"));
    let pages: serde_json::Value = serde_json::from_str(&index.body).unwrap();
    assert_eq!(pages[0]["t"], "Keyboard build: Corne v4", "newest first");
    let preview = atrium
        .get("/wiki/preview.json?href=/wiki/homelab/tailnet-acls.md")
        .await;
    assert_eq!(preview.status, 200);
    let preview: serde_json::Value = serde_json::from_str(&preview.body).unwrap();
    assert_eq!(preview["title"], "Tailnet ACLs");
    assert!(
        preview["text"]
            .as_str()
            .unwrap()
            .starts_with("Only laurel may reach")
    );
    let missing = atrium.get("/wiki/preview.json?href=/wiki/nothing.md").await;
    assert_eq!(missing.status, 404);
}

#[tokio::test]
async fn the_palette_finds_its_pages_at_the_url_the_page_names() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    let home = atrium.get("/").await.body;
    let named = attrs(&home, "[data-module~=palette]", "data-pages");
    assert_eq!(
        named,
        ["/wiki/index.json", "/wiki/index.json"],
        "the Jump link and the command line"
    );
    let pages: serde_json::Value = serde_json::from_str(&atrium.get(&named[0]).await.body).unwrap();
    let pages = pages.as_array().unwrap();
    assert!(pages.len() > 10, "{pages:?}");
    for page in pages {
        let page = page.as_object().unwrap();
        let mut keys: Vec<_> = page.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["href", "m", "path", "t"], "{page:?}");
        assert!(page.values().all(serde_json::Value::is_string), "{page:?}");
        assert!(page["href"].as_str().unwrap().starts_with("/wiki/page/"));
    }
}

#[tokio::test]
async fn a_page_names_where_its_links_previews_are_fetched() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    let pages: serde_json::Value =
        serde_json::from_str(&atrium.get("/wiki/index.json").await.body).unwrap();
    let backups = pages
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["path"] == "homelab/backup-rotation")
        .unwrap();
    let page = atrium.get(backups["href"].as_str().unwrap()).await.body;
    assert_eq!(
        texts(&page, "script#previews"),
        Vec::<String>::new(),
        "no inline previews"
    );
    assert_eq!(
        texts(&page, "article .resume-note[hidden] span"),
        [""],
        "the slot the reading module fills"
    );
    let route = &attrs(&page, "article[data-module~=preview]", "data-previews")[0];
    let links = attrs(&page, "article [data-preview]", "data-preview");
    assert_ne!(links, Vec::<String>::new());
    for link in links {
        let reply = atrium.get(&format!("{route}?href={link}")).await;
        assert_eq!(reply.status, 200, "{link}");
    }
}

#[tokio::test]
async fn the_whole_entrance_plays_once_and_the_next_page_comes_in_short() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "owner"]).await;
    let first = atrium.get("/").await;
    assert_eq!(attrs(&first.body, "html", "data-entrance"), ["full"]);
    let cookie = first.header("set-cookie").unwrap();
    let cookie = cookie.split(';').next().unwrap();
    assert!(cookie.starts_with("atrium-visit="), "{cookie}");
    let next = atrium.get_with("/wiki", &[("cookie", cookie)]).await;
    assert_eq!(attrs(&next.body, "html", "data-entrance"), ["short"]);
}

#[tokio::test]
async fn search_results_come_in_the_slots_the_swap_module_replaces() {
    let atrium = Atrium::owner().await;
    let reply = atrium.get("/wiki/search?q=harbours").await;
    assert_eq!(
        attrs(&reply.body, "form[data-module=swap]", "action"),
        ["/wiki/search"]
    );
    assert_eq!(texts(&reply.body, "[data-swap-target] h2").len(), 1);
    assert_eq!(texts(&reply.body, "[data-count]").len(), 1);
    assert_eq!(texts(&reply.body, "template"), Vec::<String>::new());
}
