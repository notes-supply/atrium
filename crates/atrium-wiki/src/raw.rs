//! A file's bytes as Mneme's `/v1/raw` sends them, for the stand-ins that play Mneme in the
//! tests and the demo: the type, disposition and policy its kind gets, a strong `ETag`, one
//! range, `If-Range`, `If-None-Match` and `HEAD`, as `docs/api.md` in Mneme describes them.

use std::fmt::Write;

use axum::body::{Body, Bytes};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, IF_NONE_MATCH, IF_RANGE, RANGE,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// What RFC 8187 escapes in a name.
const NAME: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

/// Mneme's policy for every kind of file but PDF.
const SANDBOXED: &str = "default-src 'none'; style-src 'unsafe-inline'; sandbox";
/// Mneme's policy for a PDF, whose viewer a sandbox would turn off.
const PDF: &str = "default-src 'none'; style-src 'unsafe-inline'";
/// Mneme's policy for an error.
const ERROR: &str = "default-src 'none'; sandbox";

/// The type a path's bytes are sent with, from its extension alone, and whether a browser
/// may show them in place, as Mneme's table has it.
#[must_use]
pub fn media(path: &str) -> (&'static str, bool) {
    let extension = path
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let media = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "weba" => "audio/webm",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "txt" | "md" | "csv" | "tsv" | "json" | "yaml" | "yml" | "toml" | "log" | "ini"
        | "conf" => "text/plain; charset=utf-8",
        _ => return ("application/octet-stream", false),
    };
    (media, true)
}

/// A strong tag for some bytes, 40 hexadecimal characters as Git's ids are.
#[must_use]
pub fn tag(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(42);
    out.push('"');
    for round in 0..3_u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ round;
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
        let _ = write!(out, "{hash:016x}");
    }
    out.truncate(41);
    out.push('"');
    out
}

/// An error as Mneme answers one on this route.
#[must_use]
pub fn error(status: StatusCode, why: &str) -> Response {
    (
        status,
        [
            (CONTENT_TYPE, "application/json"),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (CONTENT_SECURITY_POLICY, ERROR),
        ],
        serde_json::json!({ "error": why }).to_string(),
    )
        .into_response()
}

/// The one range a `Range` header asks of `size` bytes: `None` for the whole file, as Mneme
/// answers several ranges or one it cannot read, and `Err` for one that holds no byte.
fn range(asked: &str, size: u64) -> Option<Result<(u64, u64), ()>> {
    let spec = asked.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start, end) = spec.trim().split_once('-')?;
    let (first, last) = match (start, end) {
        ("", suffix) => {
            let suffix: u64 = suffix.parse().ok()?;
            if suffix == 0 {
                return Some(Err(()));
            }
            (size.saturating_sub(suffix), size.checked_sub(1)?)
        }
        (start, "") => (start.parse().ok()?, size.saturating_sub(1)),
        (start, end) => {
            let (start, end): (u64, u64) = (start.parse().ok()?, end.parse().ok()?);
            if end < start {
                return None;
            }
            (start, end.min(size.saturating_sub(1)))
        }
    };
    if first >= size {
        return Some(Err(()));
    }
    Some(Ok((first, last)))
}

/// `Content-Disposition` for a file's name: inline or as a download, the name once in ASCII
/// and again in full when it is not plain ASCII.
fn disposition(path: &str, inline: bool) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let plain: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut value = format!(
        "{}; filename=\"{plain}\"",
        if inline { "inline" } else { "attachment" }
    );
    if plain != name {
        let _ = write!(
            value,
            "; filename*=UTF-8''{}",
            utf8_percent_encode(name, NAME)
        );
    }
    value
}

/// The answer for `bytes`, the file at `path`, at a commit when `pinned`.
#[must_use]
pub fn answer(
    method: &Method,
    headers: &HeaderMap,
    path: &str,
    bytes: Bytes,
    pinned: bool,
) -> Response {
    let (media, inline) = media(path);
    let tag = tag(&bytes);
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let mut out = HeaderMap::new();
    let mut put = |name, value: &str| {
        if let Ok(value) = HeaderValue::from_str(value) {
            out.insert(name, value);
        }
    };
    put(ETAG, &tag);
    put(
        CACHE_CONTROL,
        if pinned {
            "max-age=31536000, immutable"
        } else {
            "no-cache"
        },
    );
    put(
        CONTENT_SECURITY_POLICY,
        if media == "application/pdf" {
            PDF
        } else {
            SANDBOXED
        },
    );
    put(ACCEPT_RANGES, "bytes");
    put(X_CONTENT_TYPE_OPTIONS, "nosniff");
    let named = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    if named(IF_NONE_MATCH.as_str())
        .is_some_and(|known| known.split(',').any(|known| known.trim() == tag))
    {
        return (StatusCode::NOT_MODIFIED, out).into_response();
    }
    let ranged = named(RANGE.as_str())
        .filter(|_| named(IF_RANGE.as_str()).is_none_or(|known| known == tag))
        .and_then(|asked| range(asked, size));
    let (status, body) = match ranged {
        Some(Err(())) => {
            let mut refused = error(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "the file has no byte in this range",
            );
            if let Ok(value) = HeaderValue::from_str(&format!("bytes */{size}")) {
                refused.headers_mut().insert(CONTENT_RANGE, value);
            }
            return refused;
        }
        Some(Ok((first, last))) => {
            put(CONTENT_RANGE, &format!("bytes {first}-{last}/{size}"));
            let (first, last) = (
                usize::try_from(first).unwrap_or(0),
                usize::try_from(last).unwrap_or(0),
            );
            (StatusCode::PARTIAL_CONTENT, bytes.slice(first..=last))
        }
        None => (StatusCode::OK, bytes),
    };
    put(CONTENT_TYPE, media);
    put(CONTENT_DISPOSITION, &disposition(path, inline));
    put(CONTENT_LENGTH, &body.len().to_string());
    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(body)
    };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = out;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_one_span_of_the_file() {
        assert_eq!(range("bytes=10-19", 100), Some(Ok((10, 19))));
        assert_eq!(range("bytes=90-", 100), Some(Ok((90, 99))));
        assert_eq!(range("bytes=-10", 100), Some(Ok((90, 99))));
        assert_eq!(range("bytes=95-200", 100), Some(Ok((95, 99))));
        assert_eq!(range("bytes=100-200", 100), Some(Err(())));
        assert_eq!(range("bytes=0-1,5-6", 100), None);
        assert_eq!(range("lines=1-2", 100), None);
    }

    #[test]
    fn a_name_that_is_not_ascii_is_given_twice() {
        assert_eq!(
            disposition("brand/Årets logga 2026.png", true),
            "inline; filename=\"_rets logga 2026.png\"; filename*=UTF-8''%C3%85rets%20logga%202026.png"
        );
        assert_eq!(
            disposition("kit.zip", false),
            "attachment; filename=\"kit.zip\""
        );
    }
}
