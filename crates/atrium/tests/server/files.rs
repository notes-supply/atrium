//! The wiki's files in front of Mneme's example responses and the demo: a file's bytes with
//! Mneme's own headers and policy, the list by folder, a file's page, and a Mneme that lists
//! no files.

use std::time::{Duration, Instant};

use atrium_wiki::stub::Stub;
use reqwest::Method;

use crate::html::{attrs, check, rows, texts};
use crate::support::Atrium;

const LOGO: &str = "/wiki/raw/brand/logo.svg";
const SANDBOX: &str = "default-src 'none'; style-src 'unsafe-inline'; sandbox";
const HEAD: &str = "faf510a1fb457f1af5496ca64dea77ed68ddb2f8";

#[tokio::test]
async fn a_files_bytes_come_with_mnemes_headers_and_its_policy() {
    let atrium = Atrium::member().await;
    let reply = atrium.get(LOGO).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 62, "as long as the listing says");
    let direct = reqwest::get(format!("{}/v1/raw/brand/logo.svg", atrium.mneme.base))
        .await
        .unwrap();
    for name in [
        "etag",
        "content-type",
        "content-length",
        "content-disposition",
        "accept-ranges",
        "x-content-type-options",
        "content-security-policy",
    ] {
        assert_eq!(
            reply.header(name),
            direct.headers().get(name).and_then(|v| v.to_str().ok()),
            "{name}"
        );
    }
    assert_eq!(reply.header("content-security-policy"), Some(SANDBOX));
    assert_eq!(reply.header("content-type"), Some("image/svg+xml"));
    assert_eq!(reply.header("cache-control"), Some("private, no-cache"));
    assert!(
        reply.settings_cookies().is_empty() && reply.header("set-cookie").is_none(),
        "a file is not a page of the visit"
    );
    assert!(
        atrium
            .mneme
            .requests()
            .contains(&"/v1/raw/brand/logo.svg".to_owned())
    );

    let named = atrium
        .get("/wiki/raw/brand/%C3%85rets%20logga%202026.png")
        .await;
    assert_eq!(named.status, 200);
    assert_eq!(
        named.header("content-disposition"),
        Some(
            "inline; filename=\"_rets logga 2026.png\"; filename*=UTF-8''%C3%85rets%20logga%202026.png"
        )
    );
    let pinned = atrium.get(&format!("{LOGO}?at={HEAD}")).await;
    assert_eq!(
        pinned.header("cache-control"),
        Some("private, max-age=31536000, immutable")
    );
    assert!(
        atrium
            .mneme
            .requests()
            .contains(&format!("/v1/raw/brand/logo.svg?at={HEAD}"))
    );
}

#[tokio::test]
async fn a_file_without_a_policy_of_its_own_gets_mnemes_not_the_pages() {
    let atrium = Atrium::member().await;
    let reply = atrium.get("/wiki/raw/bare.svg").await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-security-policy"), Some(SANDBOX));
}

#[tokio::test]
async fn ranges_tags_and_head_pass_through() {
    let atrium = Atrium::member().await;
    let whole = atrium.get(LOGO).await;
    let tag = whole.header("etag").unwrap().to_owned();

    let part = atrium.get_with(LOGO, &[("range", "bytes=10-19")]).await;
    assert_eq!(part.status, 206);
    assert_eq!(part.header("content-range"), Some("bytes 10-19/62"));
    assert_eq!(part.body, whole.body[10..20]);

    let resumed = atrium
        .get_with(LOGO, &[("range", "bytes=50-"), ("if-range", &tag)])
        .await;
    assert_eq!(resumed.status, 206);
    assert_eq!(resumed.body, whole.body[50..]);
    let changed = atrium
        .get_with(LOGO, &[("range", "bytes=50-"), ("if-range", "\"other\"")])
        .await;
    assert_eq!(changed.status, 200, "the file changed, so all of it");
    assert_eq!(changed.body, whole.body);

    let kept = atrium.get_with(LOGO, &[("if-none-match", &tag)]).await;
    assert_eq!(kept.status, 304);
    assert_eq!(kept.header("etag"), Some(tag.as_str()));
    assert_eq!(kept.body, "");

    let none = atrium.get_with(LOGO, &[("range", "bytes=100-")]).await;
    assert_eq!(none.status, 416);
    assert_eq!(none.header("content-range"), Some("bytes */62"));
    assert_eq!(none.body, "", "Mneme's reason is not relayed");

    let head = atrium.send(Method::HEAD, LOGO, &[], "").await;
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length"), Some("62"));
    assert_eq!(head.body, "");
    assert!(
        atrium
            .mneme
            .requests()
            .iter()
            .filter(|request| request.as_str() == "/v1/raw/brand/logo.svg")
            .count()
            >= 6
    );
}

#[tokio::test]
async fn a_download_longer_than_one_call_is_not_cut_short() {
    // The calls give up after a second; this file takes two to arrive.
    let atrium = Atrium::member().await;
    let start = Instant::now();
    let reply = atrium.get("/wiki/raw/slow.bin").await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body.len(), 5120);
    assert!(start.elapsed() > Duration::from_millis(1500));
    assert!(reply.body.ends_with(&"e".repeat(1024)));
}

#[tokio::test]
async fn mnemes_errors_become_atriums_own_pages() {
    let atrium = Atrium::member().await;
    let missing = atrium.get("/wiki/raw/brand/nothing.png").await;
    assert_eq!(missing.status, 404);
    check("/wiki/raw/brand/nothing.png", &missing.body);
    assert!(!missing.body.contains("nothing is at this path"));
    assert!(
        missing
            .header("content-security-policy")
            .unwrap()
            .starts_with("default-src 'none'; script-src 'self'"),
        "Atrium's page, under Atrium's policy"
    );

    let busy = atrium.get("/wiki/raw/busy.bin").await;
    assert_eq!(busy.status, 503);
    assert_eq!(busy.header("retry-after"), Some("30"));
    assert!(busy.body.contains("The wiki is out of reach just now."));
    assert!(!busy.body.contains("delta"));

    let unread = atrium.get(&format!("{LOGO}?at=main")).await;
    assert_eq!(unread.status, 400);
    check(LOGO, &unread.body);

    let away = Atrium::start(Stub::Down, &["--viewer", "member"]).await;
    let reply = away.get(LOGO).await;
    assert_eq!(reply.status, 503);
    assert!(reply.body.contains("The wiki is out of reach just now."));
}

#[tokio::test]
async fn a_deleted_files_bytes_name_what_the_record_remembers() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let gone = atrium.get("/wiki/raw/brand/old-mark.png").await;
    assert_eq!(gone.status, 404);
    assert_eq!(texts(&gone.body, ".gone .ttl-gone"), ["old-mark.png"]);
    assert_eq!(
        attrs(&gone.body, ".gone a", "href"),
        ["/wiki/files/brand/old-mark.png"]
    );
    assert!(texts(&gone.body, ".gone .diff-sum")[0].ends_with("by rowan"));
}

#[tokio::test]
async fn the_logos_first_export_keeps_its_script_behind_mnemes_sandbox() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let page = atrium.get("/wiki/files/brand/logo.svg").await.body;
    let first = attrs(&page, ".file-events a[download]", "href")
        .pop()
        .unwrap();
    let reply = atrium.get(&first).await;
    assert!(reply.body.contains("<script>"), "{}", reply.body);
    assert_eq!(reply.header("content-security-policy"), Some(SANDBOX));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
}

#[tokio::test]
async fn an_anonymous_listener_serves_no_file() {
    let atrium = Atrium::start(Stub::Fixtures, &["--viewer", "anonymous"]).await;
    for path in [LOGO, "/wiki/files", "/wiki/files/brand/logo.svg"] {
        assert_eq!(atrium.get(path).await.status, 404, "{path}");
    }
    assert!(
        atrium.mneme.requests().is_empty(),
        "{:?}",
        atrium.mneme.requests()
    );
}

#[tokio::test]
async fn the_files_are_listed_by_folder_with_sort_and_a_filter() {
    let atrium = Atrium::member().await;
    let top = atrium.get("/wiki/files").await;
    assert_eq!(top.status, 200);
    check("/wiki/files", &top.body);
    assert_eq!(texts(&top.body, "h1"), ["Files"]);
    assert_eq!(
        texts(&top.body, "tbody a.ttl"),
        ["brand/", "brandy.txt"],
        "a folder first, then the files at the top"
    );
    assert_eq!(texts(&top.body, ".wiki-switch a"), ["Pages 5", "Files 6"]);

    let brand = atrium.get("/wiki/files?under=brand").await;
    assert_eq!(texts(&brand.body, "h1"), ["brand"]);
    assert_eq!(
        rows(&brand.body, "tbody tr"),
        [
            [
                "DIR, a folder",
                "archive/one file",
                "2.9 KiB",
                "1 Sep",
                "Writer"
            ],
            ["SVG", "logo.svg", "62 B", "1 Sep", "Writer"],
            ["PNG", "mark.png", "12 KiB", "1 Sep", "Writer"],
            ["TXT", "palette.txt", "26 B", "1 Sep", "Writer"],
            ["PNG", "Årets logga 2026.png", "700 B", "1 Sep", "Writer"],
        ]
        .map(|row| row.map(str::to_owned)),
    );
    let by_size = atrium.get("/wiki/files?under=brand&sort=size").await;
    assert_eq!(
        texts(&by_size.body, "tbody a.ttl"),
        [
            "archive/",
            "mark.png",
            "Årets logga 2026.png",
            "logo.svg",
            "palette.txt"
        ]
    );
    assert_eq!(texts(&by_size.body, "th[aria-sort=descending]"), ["Size"]);

    let filtered = atrium.get("/wiki/files?under=brand&q=logga").await;
    assert_eq!(filtered.status, 200);
    assert!(
        atrium.mneme.requests().contains(
            &"/v1/files?offset=0&limit=16&under=brand&q=logga&sort=path&order=asc".to_owned()
        ),
        "{:?}",
        atrium.mneme.requests()
    );
    assert_eq!(
        attrs(&filtered.body, "form.file-find input[name=under]", "value"),
        ["brand"]
    );
    assert_eq!(atrium.get("/wiki/files?sort=colour").await.status, 400);
}

#[tokio::test]
async fn a_files_page_shows_it_and_every_version() {
    let atrium = Atrium::member().await;
    let reply = atrium.get("/wiki/files/brand/archive/Kit%202026.zip").await;
    assert_eq!(reply.status, 200);
    check("/wiki/files/brand/archive/Kit 2026.zip", &reply.body);
    assert_eq!(texts(&reply.body, "h1"), ["Kit 2026.zip"]);
    assert_eq!(
        attrs(&reply.body, ".crumbs a", "href"),
        [
            "/wiki/files",
            "/wiki/files?under=brand",
            "/wiki/files?under=brand%2Farchive"
        ]
    );
    assert_eq!(
        texts(&reply.body, ".file-actions a.btn"),
        ["Download2.9 KiB"]
    );
    assert_eq!(
        attrs(&reply.body, ".file-actions a.btn", "href"),
        ["/wiki/raw/brand/archive/Kit%202026.zip"]
    );
    let events = rows(&reply.body, ".file-events tbody tr");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0][2], "movedfrom brand/kit.zip");
    assert_eq!(
        attrs(&reply.body, ".file-events a[download]", "href"),
        [
            "/wiki/raw/brand/archive/Kit%202026.zip?at=1b6b2d8187460973c73127a3e37a7e2cab811950",
            "/wiki/raw/brand/kit.zip?at=a75f4a67d9148128bb1b4cc2f494d997cef810e7"
        ]
    );
    assert_eq!(
        atrium.get("/wiki/files/brand/nothing.png").await.status,
        404
    );
}

#[tokio::test]
async fn a_deleted_files_page_keeps_its_history_and_old_downloads() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let reply = atrium.get("/wiki/files/brand/old-mark.png").await;
    assert_eq!(reply.status, 200);
    assert!(texts(&reply.body, ".page-head .sub")[0].starts_with("Deleted "));
    assert!(
        texts(&reply.body, ".file-view").is_empty(),
        "nothing to show now"
    );
    let old = attrs(&reply.body, ".file-events a[download]", "href");
    assert_eq!(old.len(), 1, "the deletion has nothing to download");
    let bytes = atrium.get(&old[0]).await;
    assert_eq!(bytes.status, 200);
    assert_eq!(bytes.header("content-type"), Some("image/png"));
}

#[tokio::test]
async fn a_path_a_file_moved_away_from_leads_to_where_it_is() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let changes = atrium.get("/wiki/changes?before=1794178800").await;
    let created = attrs(&changes.body, "li[data-file] a.ttl", "href");
    assert!(
        created.contains(&"/wiki/files/brand/guidelines-draft.pdf".to_owned()),
        "an older change links the path the file had then: {created:?}"
    );
    let page = atrium.get("/wiki/files/brand/guidelines-draft.pdf").await;
    assert_eq!(page.status, 303);
    assert_eq!(
        page.header("location"),
        Some("/wiki/files/brand/print/guidelines.pdf")
    );
    let bytes = atrium.get("/wiki/raw/brand/guidelines-draft.pdf").await;
    assert_eq!(bytes.status, 303);
    assert_eq!(
        bytes.header("location"),
        Some("/wiki/raw/brand/print/guidelines.pdf")
    );
    let then = atrium
        .get(&format!(
            "/wiki/raw/brand/guidelines-draft.pdf?at={}",
            "0123456789abcdef0123456789abcdef01234567"
        ))
        .await;
    assert_eq!(
        then.status, 404,
        "at a commit, the path's own bytes or none"
    );
    let deleted = atrium.get("/wiki/raw/brand/old-mark.png").await;
    assert_eq!(deleted.status, 404, "a deletion is not a move");
    assert_eq!(atrium.get("/wiki/files/brand/never.pdf").await.status, 404);
}

#[tokio::test]
async fn a_text_files_page_shows_its_start() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let reply = atrium.get("/wiki/files/brand/palette.txt").await;
    assert!(
        texts(&reply.body, ".text-preview code")[0].starts_with("Saturday Loaves, the palette")
    );
    let pdf = atrium.get("/wiki/files/brand/print/guidelines.pdf").await;
    assert_eq!(
        texts(&pdf.body, ".open-preview a.btn"),
        ["Open guidelines.pdf"]
    );
    let image = atrium.get("/wiki/files/brand/logo.svg").await;
    assert_eq!(
        attrs(&image.body, ".img-preview img", "src"),
        ["/wiki/raw/brand/logo.svg"]
    );
}

#[tokio::test]
async fn a_page_shows_its_images_and_links_its_files_with_their_size() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let found = atrium.get("/wiki/brand/brand-kit.md").await;
    let page = atrium.get(found.header("location").unwrap()).await;
    assert_eq!(
        attrs(&page.body, "article img", "src"),
        ["/wiki/raw/brand/logo.svg"]
    );
    assert_eq!(attrs(&page.body, "article img", "loading"), ["lazy"]);
    assert_eq!(
        texts(&page.body, "article a.file-link"),
        [
            "The whole kit, ZIP, 1.2 KiB",
            "The guidelines, PDF, 756 B",
            "The palette, TXT, 227 B",
            "square mark, PNG, 157 B"
        ]
    );
    for href in attrs(&page.body, "article img, article a.file-link", "src")
        .into_iter()
        .chain(attrs(&page.body, "article a.file-link", "href"))
    {
        assert_eq!(atrium.get(&href).await.status, 200, "{href}");
    }
}

#[tokio::test]
async fn files_come_among_the_changes_and_in_the_palettes_list() {
    let atrium = Atrium::start(Stub::Demo, &["--viewer", "member"]).await;
    let home = atrium.get("/").await;
    let tile = texts(&home.body, ".tile.changes li");
    assert!(tile[1].starts_with("Wed 08:30kit.zip ZIP"), "{tile:?}");
    assert_eq!(
        attrs(&home.body, ".tile.changes li[data-file] a.ttl", "href"),
        ["/wiki/files/brand/kit.zip"]
    );
    let changes = atrium.get("/wiki/changes").await;
    assert_eq!(
        texts(&changes.body, ".days li[data-file] .kind"),
        [
            "file in brand, a new version",
            "file in brand, a new version",
            "file moved from brand/guidelines-draft.pdf",
            "file deleted, was brand/old-mark.png"
        ]
    );
    let index: serde_json::Value =
        serde_json::from_str(&atrium.get("/wiki/index.json").await.body).unwrap();
    let names: Vec<_> = index
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["g"] == "file")
        .map(|item| item["t"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "kit.zip",
            "logo.svg",
            "guidelines.pdf",
            "palette.txt",
            "mark.png",
            "laurel-rack.svg"
        ]
    );
}

#[tokio::test]
async fn a_mneme_without_files_leaves_the_wiki_as_it_was() {
    let atrium = Atrium::start(Stub::Older, &["--viewer", "member"]).await;
    let list = atrium.get("/wiki").await;
    assert_eq!(list.status, 200);
    assert_eq!(texts(&list.body, ".wiki-switch"), Vec::<String>::new());
    let files = atrium.get("/wiki/files").await;
    assert_eq!(files.status, 200);
    assert_eq!(
        texts(&files.body, ".page-head .sub"),
        ["This wiki's Mneme does not list its files. A newer Mneme does."]
    );
    let index = atrium.get("/wiki/index.json").await;
    assert_eq!(index.status, 200);
    assert!(!index.body.contains("\"g\""), "{}", index.body);
    let changes = atrium.get("/wiki/changes").await;
    assert_eq!(changes.status, 200);
    assert_eq!(texts(&changes.body, ".days li").len(), 13);
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert_eq!(texts(&home.body, ".tile.changes ol li").len(), 8);
    assert!(!home.body.contains("briefly away"), "{}", home.body);
}
