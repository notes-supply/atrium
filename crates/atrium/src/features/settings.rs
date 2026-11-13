//! The settings page, which every viewer may see: the theme, the field and the single-letter
//! keys. Each is a cookie the server draws every page with. A form posts one choice, and the
//! answer sets its cookie, as the browser module does, and leads back to the page. A post
//! from another site is refused.

use atrium_core::{Access, Choice, Feature, Prefs};
use atrium_ui::Shell;
use atrium_ui::settings::{self, PATH};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header::{HOST, LOCATION, ORIGIN, REFERRER_POLICY, SET_COOKIE};
use axum::http::uri::Authority;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// The most a form of one choice sends: `intensity=normal` and the like.
const LIMIT: usize = 256;

pub struct Settings;

impl Feature for Settings {
    fn id(&self) -> &'static str {
        "settings"
    }

    fn name(&self) -> &'static str {
        "Settings"
    }

    /// Display settings are not private, and a reader sets only their own.
    fn access(&self) -> Access {
        Access::Public
    }

    fn routes(&self) -> Router {
        Router::new().route(PATH, get(page).post(choose))
    }
}

/// The page. Under every other page's `no-referrer`, a browser sends `Origin: null` with the
/// page's own posts, which [`same_site`] cannot tell from another site's; `same-origin` sends
/// the page's origin to the page's own site, and still nothing to any other.
async fn page(shell: Shell) -> Response {
    let mut response = shell.page(settings::page(&shell));
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    response
}

/// Takes one choice, `setting=value`, sets its cookie and answers 303 to the page. A post
/// from another site gets 403, and anything else 400, without saying back what was sent.
async fn choose(headers: HeaderMap, body: Body) -> Response {
    if !same_site(&headers) {
        return (StatusCode::FORBIDDEN, "not from this site\n").into_response();
    }
    let choice = to_bytes(body, LIMIT)
        .await
        .ok()
        .and_then(|bytes| serde_urlencoded::from_bytes::<Vec<(String, String)>>(&bytes).ok())
        .and_then(|pairs| match &pairs[..] {
            [(setting, value)] => Choice::parse(setting, value),
            _ => None,
        });
    match choice {
        Some(choice) => (
            StatusCode::SEE_OTHER,
            [(LOCATION, PATH.to_owned()), (SET_COOKIE, cookie(choice))],
        )
            .into_response(),
        None => (StatusCode::BAD_REQUEST, "unknown setting\n").into_response(),
    }
}

/// Whether a post comes from Atrium's own pages: not when the browser says it is
/// `cross-site`, nor when its `Origin` names another host or port than its `Host`. A request
/// with neither header, as from curl or a very old browser, is let through; the host check
/// has already admitted its `Host`.
fn same_site(headers: &HeaderMap) -> bool {
    let header = |name| headers.get(name).and_then(|value| value.to_str().ok());
    if header("sec-fetch-site").is_some_and(|site| site.eq_ignore_ascii_case("cross-site")) {
        return false;
    }
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let origin = origin
        .to_str()
        .ok()
        .and_then(|origin| origin.parse::<Uri>().ok());
    let host = header(HOST.as_str()).and_then(|host| host.parse::<Authority>().ok());
    let (Some(origin), Some(host)) = (origin, host) else {
        return false;
    };
    let default = match origin.scheme_str() {
        Some("http") => 80,
        Some("https") => 443,
        _ => return false,
    };
    origin.authority().is_some_and(|from| {
        from.host().eq_ignore_ascii_case(host.host())
            && from.port_u16().unwrap_or(default) == host.port_u16().unwrap_or(default)
    })
}

/// The cookie as the browser module writes it (`dom::remember`), so either may replace the
/// other's: the same name, path, lifetime and `SameSite`, and no `HttpOnly`, since the module
/// writes it too. Following the system clears the theme's.
fn cookie(choice: Choice) -> String {
    let (value, age) = choice.kept().map_or(("", 0), |value| (value, Prefs::KEEP));
    format!(
        "{}={value}; Path=/; Max-Age={age}; SameSite=Lax",
        choice.cookie()
    )
}

#[cfg(test)]
mod tests {
    use atrium_core::{Intensity, Theme};

    use super::*;

    fn same_site_with(pairs: &[(&'static str, &'static str)]) -> bool {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, value.parse().unwrap());
        }
        same_site(&headers)
    }

    #[test]
    fn a_post_from_another_site_is_refused() {
        for allowed in [
            &[("host", "localhost:8080")][..],
            &[
                ("host", "localhost:8080"),
                ("origin", "http://localhost:8080"),
            ],
            &[
                ("host", "LOCALHOST:8080"),
                ("origin", "http://localhost:8080"),
            ],
            &[
                ("host", "atrium.example"),
                ("origin", "https://atrium.example"),
            ],
            &[
                ("host", "atrium.example"),
                ("origin", "https://atrium.example:443"),
            ],
            &[("host", "[::1]:8080"), ("origin", "http://[::1]:8080")],
            &[("host", "localhost"), ("sec-fetch-site", "same-origin")],
            &[("host", "localhost"), ("sec-fetch-site", "none")],
        ] {
            assert!(same_site_with(allowed), "{allowed:?}");
        }
        for refused in [
            &[("host", "localhost:8080"), ("sec-fetch-site", "cross-site")][..],
            &[("host", "localhost:8080"), ("sec-fetch-site", "Cross-Site")],
            &[
                ("host", "localhost:8080"),
                ("origin", "http://localhost:8080"),
                ("sec-fetch-site", "cross-site"),
            ],
            &[
                ("host", "localhost:8080"),
                ("origin", "http://evil.example"),
            ],
            &[
                ("host", "localhost:8080"),
                ("origin", "http://localhost:8081"),
            ],
            &[("host", "localhost:8080"), ("origin", "http://localhost")],
            &[
                ("host", "atrium.example"),
                ("origin", "http://atrium.example.evil"),
            ],
            &[("host", "localhost:8080"), ("origin", "null")],
            &[
                ("host", "localhost:8080"),
                ("origin", "ftp://localhost:8080"),
            ],
            &[("origin", "http://localhost:8080")],
        ] {
            assert!(!same_site_with(refused), "{refused:?}");
        }
    }

    #[test]
    fn each_choice_sets_its_cookie_for_a_year_and_the_system_clears_the_theme() {
        assert_eq!(
            cookie(Choice::Theme(Some(Theme::Dark))),
            "atrium-theme=dark; Path=/; Max-Age=31536000; SameSite=Lax"
        );
        assert_eq!(
            cookie(Choice::Intensity(Intensity::Calm)),
            "atrium-intensity=calm; Path=/; Max-Age=31536000; SameSite=Lax"
        );
        assert_eq!(
            cookie(Choice::LetterKeys(false)),
            "atrium-keys=off; Path=/; Max-Age=31536000; SameSite=Lax"
        );
        assert_eq!(
            cookie(Choice::Theme(None)),
            "atrium-theme=; Path=/; Max-Age=0; SameSite=Lax"
        );
    }
}
