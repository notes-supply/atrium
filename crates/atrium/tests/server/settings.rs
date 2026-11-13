//! The settings page: public, drawn from the reader's cookies, and a form that sets one
//! cookie at a time and leads back.

use atrium_wiki::stub::Stub;
use reqwest::Method;

use crate::html::{attrs, check, texts};
use crate::support::Atrium;

#[tokio::test]
async fn every_viewer_may_see_the_settings_and_every_page_links_them() {
    for viewer in ["member", "anonymous"] {
        let atrium = Atrium::start(Stub::Fixtures, &["--viewer", viewer]).await;
        let reply = atrium.get("/settings").await;
        assert_eq!(reply.status, 200, "{viewer}");
        check("/settings", &reply.body);
        assert_eq!(texts(&reply.body, "h1"), ["Settings"]);
        assert_eq!(
            attrs(
                &atrium.get("/no-such-page").await.body,
                ".mast a.settings-btn",
                "href"
            ),
            ["/settings"],
            "{viewer}"
        );
    }
}

#[tokio::test]
async fn the_page_presses_the_readers_choices() {
    let atrium = Atrium::member().await;
    let pressed = |body: &str| attrs(body, "button[aria-pressed=true]", "value");
    let plain = atrium.get("/settings").await.body;
    assert_eq!(pressed(&plain), ["system", "normal", "on"]);
    assert_eq!(
        attrs(&plain, "form.setting", "method"),
        ["post", "post", "post"]
    );
    assert_eq!(
        attrs(&plain, "form.setting", "action"),
        ["/settings", "/settings", "/settings"]
    );
    let chosen = atrium
        .get_with(
            "/settings",
            &[(
                "cookie",
                "atrium-theme=dark; atrium-intensity=lively; atrium-keys=off",
            )],
        )
        .await
        .body;
    assert_eq!(pressed(&chosen), ["dark", "lively", "off"]);
}

#[tokio::test]
async fn the_keyboard_lists_every_key_but_none_for_the_field() {
    let atrium = Atrium::member().await;
    let page = atrium.get("/settings").await.body;
    assert_eq!(
        texts(&page, ".key-list dd"),
        ["sections", "rows", "search", "jump", "theme"]
    );
    assert_eq!(texts(&page, ".key-list dt")[0], "1–5");
    for path in ["/", "/wiki", "/settings"] {
        let page = atrium.get(path).await.body;
        assert_eq!(
            texts(&page, "footer > *"),
            [texts(&page, ".foot-note")[0].clone()],
            "{path}: the footer is its note"
        );
        assert!(!texts(&page, "kbd").contains(&"F".to_owned()), "{path}");
    }
}

#[tokio::test]
async fn a_choice_sets_its_cookie_and_leads_back() {
    let atrium = Atrium::member().await;
    for (form, cookie) in [
        (
            "theme=dark",
            "atrium-theme=dark; Path=/; Max-Age=31536000; SameSite=Lax",
        ),
        (
            "theme=light",
            "atrium-theme=light; Path=/; Max-Age=31536000; SameSite=Lax",
        ),
        (
            "theme=system",
            "atrium-theme=; Path=/; Max-Age=0; SameSite=Lax",
        ),
        (
            "intensity=calm",
            "atrium-intensity=calm; Path=/; Max-Age=31536000; SameSite=Lax",
        ),
        (
            "keys=off",
            "atrium-keys=off; Path=/; Max-Age=31536000; SameSite=Lax",
        ),
        (
            "keys=on",
            "atrium-keys=on; Path=/; Max-Age=31536000; SameSite=Lax",
        ),
    ] {
        let reply = atrium.post("/settings", form).await;
        assert_eq!(reply.status, 303, "{form}");
        assert_eq!(reply.header("location"), Some("/settings"), "{form}");
        assert_eq!(reply.settings_cookies(), [cookie], "{form}");
        assert_eq!(reply.body, "", "{form}");
    }
}

#[tokio::test]
async fn anything_but_one_known_choice_is_refused_without_being_said_back() {
    let atrium = Atrium::member().await;
    let long = format!("theme={}", "d".repeat(4096));
    for form in [
        "",
        "theme=sepia",
        "theme=Dark",
        "intensity=system",
        "keys=yes",
        "colour=dark",
        "atrium-theme=dark",
        "theme=dark&intensity=calm",
        "theme=dark&theme=light",
        "theme",
        "theme=%3Cscript%3E",
        &long,
    ] {
        let reply = atrium.post("/settings", form).await;
        assert_eq!(reply.status, 400, "{form}");
        assert_eq!(reply.settings_cookies(), Vec::<&str>::new(), "{form}");
        assert_eq!(reply.body, "unknown setting\n", "{form}");
    }
}

#[tokio::test]
async fn only_get_and_post_are_answered() {
    let atrium = Atrium::member().await;
    for method in [Method::PUT, Method::DELETE, Method::PATCH] {
        let reply = atrium
            .send(method.clone(), "/settings", &[], "theme=dark")
            .await;
        assert_eq!(reply.status, 405, "{method}");
        assert_eq!(reply.header("allow"), Some("GET,HEAD,POST"), "{method}");
        assert_eq!(reply.settings_cookies(), Vec::<&str>::new(), "{method}");
    }
}

#[tokio::test]
async fn a_choice_naming_an_unknown_host_is_refused() {
    let atrium = Atrium::member().await;
    let reply = atrium
        .send(
            Method::POST,
            "/settings",
            &[
                ("host", "evil.example"),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            "theme=dark",
        )
        .await;
    assert_eq!(reply.status, 421);
    assert_eq!(reply.header("set-cookie"), None);
}
