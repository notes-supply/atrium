//! The settings page, which every viewer may see: the theme, the field and the single-letter
//! keys. Each is a cookie the server draws every page with. A form posts one choice, and the
//! answer sets its cookie, as the browser module does, and leads back to the page.

use atrium_core::{Access, Choice, Feature, Prefs};
use atrium_ui::Shell;
use atrium_ui::settings::{self, PATH};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::StatusCode;
use axum::http::header::{LOCATION, SET_COOKIE};
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

async fn page(shell: Shell) -> Response {
    shell.page(settings::page(&shell))
}

/// Takes one choice, `setting=value`, sets its cookie and answers 303 to the page. Anything
/// else is refused, without saying back what was sent.
async fn choose(body: Body) -> Response {
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
