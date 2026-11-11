//! A listener serves one kind of viewer, and private features exist only for the tailnet.

use atrium_wiki::stub::Stub;

use crate::html::{attrs, texts};
use crate::support::Atrium;

#[tokio::test]
async fn an_anonymous_listener_refuses_every_private_route() {
    let atrium = Atrium::start(Stub::Fixtures, &["--viewer", "anonymous"]).await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 404);
    let unknown = atrium.get("/no-such-page").await.body;
    assert_eq!(
        home.body.replace("404, /<", "404, PATH<"),
        unknown.replace("404, /no-such-page<", "404, PATH<"),
        "a refusal reads as an address with nothing at it"
    );
}

#[tokio::test]
async fn an_anonymous_listener_links_no_private_feature() {
    let atrium = Atrium::start(Stub::Fixtures, &["--viewer", "anonymous"]).await;
    let page = atrium.get("/no-such-page").await.body;
    assert_eq!(attrs(&page, "header nav a", "href"), Vec::<String>::new());
    assert_eq!(attrs(&page, "footer nav a", "href"), Vec::<String>::new());
}

#[tokio::test]
async fn the_tailnet_listener_serves_home_and_links_it() {
    let atrium = Atrium::tailnet().await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert_eq!(
        attrs(&home.body, "h1 time", "datetime").len(),
        1,
        "home is headed by the date"
    );
    assert_eq!(
        texts(&home.body, "header nav a[aria-current=page] .lb"),
        ["Home"]
    );
}

#[tokio::test]
async fn the_viewer_must_be_named() {
    use clap::Parser;
    let parse = |viewer: &[&str]| {
        atrium::Config::try_parse_from(
            [&["atrium", "--mneme-url", "http://mneme"], viewer].concat(),
        )
    };
    assert!(parse(&[]).is_err());
    assert!(parse(&["--viewer", "everyone"]).is_err());
    assert!(parse(&["--viewer", "anonymous"]).is_ok());
}
