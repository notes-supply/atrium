//! Headers, caching and the HTML rules, over every route.

use atrium_wiki::stub::Stub;

use crate::html::{attrs, check};
use crate::support::Atrium;

/// One address per route, with the status each answers.
const ROUTES: &[(&str, u16)] = &[
    ("/", 200),
    ("/no-such-page", 404),
    ("/wiki", 200),
    ("/wiki?sort=churn&order=asc", 200),
    ("/wiki?sort=colour", 400),
    ("/wiki/page/4226c96dd1386f3a", 200),
    ("/wiki/page/0123456789abcdef", 404),
    ("/wiki/page/4226c96dd1386f3a/history", 200),
    (
        "/wiki/page/4226c96dd1386f3a/at/faf510a1fb457f1af5496ca64dea77ed68ddb2f8",
        200,
    ),
    ("/wiki/page/4226c96dd1386f3a/diff", 200),
    ("/wiki/changes", 200),
    ("/wiki/search", 200),
    ("/wiki/search?q=harbours", 200),
    ("/wiki/search?q=%22open", 400),
    ("/wiki/machines", 200),
];

const CSP: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; \
    form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

#[tokio::test]
async fn every_page_keeps_the_output_rules() {
    let atrium = Atrium::tailnet().await;
    for (path, status) in ROUTES {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, *status, "{path}");
        assert_eq!(reply.header("content-security-policy"), Some(CSP), "{path}");
        assert_eq!(reply.header("cache-control"), Some("no-store"), "{path}");
        assert_eq!(
            reply.header("content-type"),
            Some("text/html; charset=utf-8"),
            "{path}"
        );
        check(path, &reply.body);
    }
}

#[tokio::test]
async fn assets_have_hashed_names_and_are_immutable() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await.body;
    let stylesheet = &attrs(&home, "link[rel=stylesheet]", "href")[0];
    let name = stylesheet.strip_prefix("/assets/").unwrap();
    let [stem, hash, extension] = name.split('.').collect::<Vec<_>>()[..] else {
        panic!("{name} is not stem.hash.extension");
    };
    assert_eq!((stem, extension), ("atrium", "css"));
    assert!(hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
    let reply = atrium.get(stylesheet).await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        reply.header("content-type"),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(atrium.get("/assets/atrium.css").await.status, 404);
}

#[tokio::test]
async fn a_page_answers_if_none_match() {
    let atrium = Atrium::tailnet().await;
    let first = atrium.get("/").await;
    let tag = first.header("etag").unwrap();
    assert!(
        tag.starts_with('"') && tag.len() == 34,
        "a strong tag: {tag}"
    );
    let again = atrium.get_with("/", &[("if-none-match", tag)]).await;
    assert_eq!(again.status, 304);
    assert_eq!(again.body, "");
    let other = atrium.get_with("/", &[("if-none-match", "\"x\"")]).await;
    assert_eq!(other.status, 200);
}

#[tokio::test]
async fn an_unknown_host_is_refused_except_for_health_checks() {
    let atrium = Atrium::tailnet().await;
    let refused = atrium.get_with("/", &[("host", "evil.example")]).await;
    assert_eq!(refused.status, 421);
    let missing = atrium
        .get_with("/no-such-page", &[("host", "evil.example")])
        .await;
    assert_eq!(missing.status, 421);
    let health = atrium
        .get_with("/healthz", &[("host", "10.0.0.7:8080")])
        .await;
    assert_eq!(health.status, 200);
}

#[tokio::test]
async fn the_palette_loads_through_one_loader_of_at_most_a_kilobyte() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await.body;
    let loader = &attrs(&home, "script", "src")[0];
    let reply = atrium.get(loader).await;
    assert_eq!(reply.status, 200);
    assert!(reply.body.len() <= 1024, "{} bytes", reply.body.len());
    assert_eq!(
        reply.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        reply.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(attrs(&home, "[data-module]", "data-module"), ["palette"]);
    let glue = atrium
        .get(&attrs(&home, "[data-module]", "data-glue")[0])
        .await;
    assert_eq!(glue.status, 200);
    assert!(
        glue.body.contains(" as default };"),
        "the default export the loader calls"
    );
    let wasm = atrium
        .get(&attrs(&home, "[data-module]", "data-wasm")[0])
        .await;
    assert_eq!(wasm.status, 200);
    assert_eq!(wasm.header("content-type"), Some("application/wasm"));
    assert!(wasm.body.as_bytes().starts_with(b"\0asm"), "a Wasm module");
}

#[tokio::test]
async fn every_command_is_also_a_plain_link() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await.body;
    assert_eq!(
        attrs(&home, "[data-module=palette] a", "href"),
        [
            "/",
            "/wiki",
            "/wiki/changes",
            "/wiki/search",
            "/wiki/machines"
        ]
    );
}

#[tokio::test]
async fn a_page_with_nothing_interactive_has_no_script() {
    let atrium = Atrium::start(Stub::Fixtures, &["--viewer", "anonymous"]).await;
    let reply = atrium.get("/").await;
    assert_eq!(reply.status, 404);
    assert!(!reply.body.contains("<script"), "{}", reply.body);
    check("/ to an anonymous viewer", &reply.body);
}
