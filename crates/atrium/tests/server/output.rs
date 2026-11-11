//! Headers, caching and the HTML rules, over every route.

use std::collections::{BTreeMap, VecDeque};

use atrium_wiki::stub::Stub;

use crate::html::{attrs, check};
use crate::support::{Atrium, Reply};

/// Addresses no link leads to, with the status each answers. Every page a link leads to is
/// found by following links from home.
const UNLINKED: &[(&str, u16)] = &[
    ("/no-such-page", 404),
    ("/wiki?sort=colour", 400),
    ("/wiki/page/0123456789abcdef", 404),
    ("/wiki/search?q=harbours", 200),
    ("/wiki/search?q=%22open", 400),
];

const CSP: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; \
    style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; \
    form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

fn keeps_the_rules(path: &str, reply: &Reply) {
    assert_eq!(reply.header("content-security-policy"), Some(CSP), "{path}");
    assert_eq!(reply.header("cache-control"), Some("no-store"), "{path}");
    assert_eq!(
        reply.header("content-type"),
        Some("text/html; charset=utf-8"),
        "{path}"
    );
    check(path, &reply.body);
}

/// Every page reachable from home by links and redirects within the site, which is every
/// route of every feature the registry lists.
async fn crawl(atrium: &Atrium) -> BTreeMap<String, u16> {
    let mut found = BTreeMap::new();
    let mut queue = VecDeque::from(["/".to_owned()]);
    while let Some(path) = queue.pop_front() {
        if found.contains_key(&path) || path.starts_with("/assets/") {
            continue;
        }
        assert!(found.len() < 500, "the crawl does not end");
        let reply = atrium.get(&path).await;
        found.insert(path.clone(), reply.status);
        let next = match reply.status {
            303 => vec![reply.header("location").unwrap().to_owned()],
            200 | 404 => {
                keeps_the_rules(&path, &reply);
                attrs(&reply.body, "a[href]", "href")
            }
            status => panic!("{path} answered {status}"),
        };
        for href in next {
            let href = href.split('#').next().unwrap_or_default();
            if href.starts_with('/') && !href.starts_with("//") {
                queue.push_back(href.to_owned());
            }
        }
    }
    found
}

#[tokio::test]
async fn every_page_keeps_the_output_rules() {
    let atrium = Atrium::tailnet().await;
    let found = crawl(&atrium).await;
    for nav in attrs(&atrium.get("/").await.body, "header nav a", "href") {
        assert_eq!(found.get(&nav), Some(&200), "{nav} was reached");
    }
    assert!(found.len() > 20, "{found:?}");
    for (path, status) in UNLINKED {
        let reply = atrium.get(path).await;
        assert_eq!(reply.status, *status, "{path}");
        keeps_the_rules(path, &reply);
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
async fn the_modules_load_through_one_loader_of_at_most_a_kilobyte() {
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
    for bundle in ["ui", "field"] {
        let glue = atrium
            .get(&attrs(&home, "script", &format!("data-{bundle}"))[0])
            .await;
        assert_eq!(glue.status, 200, "{bundle}");
        assert!(
            glue.body.contains(" as default };"),
            "{bundle}: the default export the loader calls"
        );
        let wasm = atrium
            .get(&attrs(&home, "script", &format!("data-{bundle}-wasm"))[0])
            .await;
        assert_eq!(wasm.status, 200, "{bundle}");
        assert_eq!(wasm.header("content-type"), Some("application/wasm"));
        assert!(
            wasm.body.as_bytes().starts_with(b"\0asm"),
            "{bundle}: a Wasm module"
        );
    }
}

#[tokio::test]
async fn every_section_is_a_plain_link_with_its_key() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await.body;
    assert_eq!(
        attrs(&home, ".mast .index a", "href"),
        [
            "/",
            "/wiki",
            "/wiki/changes",
            "/wiki/search",
            "/wiki/machines"
        ]
    );
    assert_eq!(
        attrs(&home, ".mast .index a", "data-key"),
        ["1", "2", "3", "4", "5"]
    );
}

#[tokio::test]
async fn a_refusal_keeps_the_output_rules() {
    let atrium = Atrium::start(Stub::Fixtures, &["--viewer", "anonymous"]).await;
    let reply = atrium.get("/").await;
    assert_eq!(reply.status, 404);
    check("/ to an anonymous viewer", &reply.body);
}
