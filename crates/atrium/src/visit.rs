//! The visit every request carries: the reader's clock in the configured zone, their settings
//! from cookies, and when they were last here, from a cookie the server keeps itself.

use atrium_core::{Entrance, Local, Prefs, Visit};
use axum::extract::{Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The cookie that holds the previous visit and this one.
const COOKIE_NAME: &str = "atrium-visit";
/// A pause longer than this ends a visit.
const VISIT_GAP: i64 = 30 * 60;
/// A reader back after longer than this sees the whole entrance again.
const ENTRANCE_GAP: i64 = 8 * 60 * 60;
/// How long the cookie is kept: a year.
const KEEP: i64 = 365 * 24 * 60 * 60;

/// Where now comes from.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    System,
    /// A fixed now and a fixed last visit, for the demo, which shows invented times.
    Fixed {
        now: Timestamp,
        last: i64,
    },
}

/// What the visit middleware is told at start.
#[derive(Debug, Clone)]
pub struct Visits {
    pub clock: Clock,
    pub zone: TimeZone,
}

/// The previous visit and the time of this one, as the cookie keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kept {
    previous: Option<i64>,
    latest: i64,
}

impl Kept {
    fn parse(value: &str) -> Option<Self> {
        let (previous, latest) = value.split_once('.')?;
        Some(Self {
            previous: match previous {
                "" => None,
                seconds => Some(seconds.parse().ok()?),
            },
            latest: latest.parse().ok()?,
        })
    }

    fn write(self) -> String {
        let previous = self.previous.map_or_else(String::new, |at| at.to_string());
        format!("{previous}.{}", self.latest)
    }

    /// The cookie after a request at `now`, and when the reader was last here before this
    /// visit. A request within the gap of the latest one belongs to the same visit.
    fn next(kept: Option<Self>, now: i64) -> (Option<i64>, Self) {
        let previous = match kept {
            None => None,
            Some(kept) if now - kept.latest <= VISIT_GAP => kept.previous,
            Some(kept) => Some(kept.latest),
        };
        (
            previous,
            Self {
                previous,
                latest: now,
            },
        )
    }
}

/// The whole entrance for a reader with no cookie or whose latest request was over
/// [`ENTRANCE_GAP`] ago, the short one otherwise.
fn entrance(kept: Option<Kept>, now: i64) -> Entrance {
    match kept {
        Some(kept) if now - kept.latest <= ENTRANCE_GAP => Entrance::Short,
        _ => Entrance::Full,
    }
}

/// Every cookie a request sends, as name and value.
fn cookies(headers: &HeaderMap) -> impl Iterator<Item = (&str, &str)> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
}

/// Whether the request reached the proxy in front of Atrium over HTTPS.
fn secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .is_some_and(|proto| proto.as_bytes().eq_ignore_ascii_case(b"https"))
}

/// Puts the [`Visit`] in the request, and keeps the visit cookie up to date.
pub async fn visit(State(visits): State<Visits>, mut request: Request, next: Next) -> Response {
    let headers = request.headers();
    let prefs = Prefs::from_cookies(cookies(headers));
    let sent = cookies(headers)
        .find(|(name, _)| *name == COOKIE_NAME)
        .and_then(|(_, value)| Kept::parse(value));
    let (now, last, kept) = match visits.clock {
        Clock::Fixed { now, last } => (
            now,
            Some(last),
            Kept {
                previous: None,
                latest: now.as_second(),
            },
        ),
        Clock::System => {
            let now = Timestamp::now();
            let (last, kept) = Kept::next(sent, now.as_second());
            (now, last, kept)
        }
    };
    let secure = secure(headers);
    let page = !request.uri().path().starts_with("/assets/");
    request.extensions_mut().insert(Visit {
        local: Local::new(now, visits.zone.clone()),
        last,
        prefs,
        entrance: entrance(sent, now.as_second()),
    });
    let mut response = next.run(request).await;
    if page {
        let cookie = format!(
            "{COOKIE_NAME}={}; Path=/; Max-Age={KEEP}; HttpOnly; SameSite=Lax{}",
            kept.write(),
            if secure { "; Secure" } else { "" }
        );
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(SET_COOKIE, value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: i64 = 1_794_390_000;

    #[test]
    fn a_first_request_knows_no_last_visit() {
        let (last, kept) = Kept::next(None, AT);
        assert_eq!(last, None);
        assert_eq!(
            kept,
            Kept {
                previous: None,
                latest: AT
            }
        );
    }

    #[test]
    fn requests_close_together_are_one_visit() {
        let first = Kept {
            previous: Some(AT - 86_400),
            latest: AT,
        };
        let (last, kept) = Kept::next(Some(first), AT + 20 * 60);
        assert_eq!(last, Some(AT - 86_400));
        assert_eq!(
            kept,
            Kept {
                previous: Some(AT - 86_400),
                latest: AT + 20 * 60
            }
        );
    }

    #[test]
    fn a_long_pause_makes_the_last_request_the_last_visit() {
        let first = Kept {
            previous: Some(AT - 86_400),
            latest: AT,
        };
        let (last, kept) = Kept::next(Some(first), AT + 2 * 60 * 60);
        assert_eq!(last, Some(AT));
        assert_eq!(kept.previous, Some(AT));
    }

    #[test]
    fn the_cookie_reads_back_and_refuses_what_it_did_not_write() {
        for kept in [
            Kept {
                previous: None,
                latest: AT,
            },
            Kept {
                previous: Some(AT - 5),
                latest: AT,
            },
        ] {
            assert_eq!(Kept::parse(&kept.write()), Some(kept));
        }
        for bad in ["", "x.1", "1", "1.y"] {
            assert_eq!(Kept::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_whole_entrance_plays_for_a_reader_arriving() {
        let latest = |latest| {
            Some(Kept {
                previous: None,
                latest,
            })
        };
        assert_eq!(entrance(None, AT), Entrance::Full);
        assert_eq!(entrance(latest(AT - 60), AT), Entrance::Short);
        assert_eq!(entrance(latest(AT - ENTRANCE_GAP), AT), Entrance::Short);
        assert_eq!(entrance(latest(AT - ENTRANCE_GAP - 1), AT), Entrance::Full);
    }

    #[test]
    fn cookies_split_across_headers_and_pairs() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=1; atrium-theme=dark"));
        headers.append(COOKIE, HeaderValue::from_static("b=2"));
        let all: Vec<_> = cookies(&headers).collect();
        assert_eq!(all, [("a", "1"), ("atrium-theme", "dark"), ("b", "2")]);
    }
}
