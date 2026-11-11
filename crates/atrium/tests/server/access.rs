//! A listener serves one kind of viewer, and private features exist only for the tailnet.

use crate::html::{attrs, texts};
use crate::support::Atrium;

#[tokio::test]
async fn an_anonymous_listener_refuses_every_private_route() {
    let atrium = Atrium::start(&["--viewer", "anonymous"]).await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 404);
    assert_eq!(home.body, atrium.get("/no-such-page").await.body);
}

#[tokio::test]
async fn an_anonymous_listener_links_no_private_feature() {
    let atrium = Atrium::start(&["--viewer", "anonymous"]).await;
    let page = atrium.get("/no-such-page").await.body;
    assert_eq!(attrs(&page, "header nav a", "href"), Vec::<String>::new());
    assert_eq!(attrs(&page, "footer nav a", "href"), Vec::<String>::new());
}

#[tokio::test]
async fn the_tailnet_listener_serves_home_and_links_it() {
    let atrium = Atrium::tailnet(&[]).await;
    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    assert_eq!(texts(&home.body, "h1"), ["Atrium"]);
    assert_eq!(
        texts(&home.body, "header nav a[aria-current=page]"),
        ["Home"]
    );
}

#[tokio::test]
async fn the_viewer_must_be_named() {
    use clap::Parser;
    assert!(atrium::Config::try_parse_from(["atrium"]).is_err());
    assert!(atrium::Config::try_parse_from(["atrium", "--viewer", "everyone"]).is_err());
}
