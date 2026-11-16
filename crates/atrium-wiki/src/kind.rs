//! What a file is, as the pages name and show it: the mark of its kind, its name and folder,
//! and how a browser can show it, which the type Mneme sends it with decides.

/// How a file's page shows it, from the `Content-Type` Mneme sends its bytes with. Mneme
/// sends nothing a browser would run as a document in place, so this follows its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Image,
    Pdf,
    Audio,
    Video,
    Text,
    /// A download only, such as a zip.
    Download,
}

impl Shown {
    #[must_use]
    pub fn of(media: &str) -> Self {
        let media = media.split(';').next().unwrap_or_default().trim();
        match media.split_once('/') {
            Some(("image", _)) => Self::Image,
            Some(("audio", _)) => Self::Audio,
            Some(("video", _)) => Self::Video,
            Some(("text", "plain")) => Self::Text,
            _ if media == "application/pdf" => Self::Pdf,
            _ => Self::Download,
        }
    }
}

/// The last part of a path: `logo.svg`.
#[must_use]
pub fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The folder a path is in, `""` at the top: `brand` for `brand/logo.svg`.
#[must_use]
pub fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The short mark of a file's kind: its extension in capitals, such as `SVG` or `ZIP`, or
/// `FILE` for a name without a short one.
#[must_use]
pub fn label(path: &str) -> String {
    name(path)
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            (1..=5).contains(&extension.len())
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map_or_else(|| "FILE".to_owned(), str::to_ascii_uppercase)
}

/// The address of a file's page.
#[must_use]
pub fn page_href(path: &str) -> String {
    format!("/wiki/files/{}", crate::markdown::path_href(path))
}

/// The address of a file's bytes, as they are now or at `at`.
#[must_use]
pub fn raw_href(path: &str, at: Option<&str>) -> String {
    let mut href = format!("/wiki/raw/{}", crate::markdown::path_href(path));
    if let Some(at) = at {
        href.push_str("?at=");
        href.push_str(at);
    }
    href
}

/// The list of files in `folder`, `""` for the top.
#[must_use]
pub fn folder_href(folder: &str) -> String {
    if folder.is_empty() {
        "/wiki/files".to_owned()
    } else {
        format!("/wiki/files?under={}", atrium_ui::words::query(folder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_is_shown_as_its_type_allows() {
        assert_eq!(Shown::of("image/svg+xml"), Shown::Image);
        assert_eq!(Shown::of("application/pdf"), Shown::Pdf);
        assert_eq!(Shown::of("audio/mpeg"), Shown::Audio);
        assert_eq!(Shown::of("video/mp4"), Shown::Video);
        assert_eq!(Shown::of("text/plain; charset=utf-8"), Shown::Text);
        assert_eq!(Shown::of("text/html"), Shown::Download);
        assert_eq!(Shown::of("application/octet-stream"), Shown::Download);
    }

    #[test]
    fn the_mark_is_a_short_extension() {
        assert_eq!(label("brand/logo.svg"), "SVG");
        assert_eq!(label("brand/archive/Kit 2026.zip"), "ZIP");
        assert_eq!(label("LICENSE"), "FILE");
        assert_eq!(label("a.verylongext"), "FILE");
        assert_eq!(label("dir.d/notes"), "FILE");
    }

    #[test]
    fn addresses_escape_each_segment() {
        assert_eq!(
            raw_href("brand/Årets logga.png", None),
            "/wiki/raw/brand/%C3%85rets%20logga.png"
        );
        assert_eq!(
            page_href("brand/a#b?.zip"),
            "/wiki/files/brand/a%23b%3F.zip"
        );
        assert_eq!(raw_href("a.svg", Some("abc")), "/wiki/raw/a.svg?at=abc");
        assert_eq!(folder_href("brand/print"), "/wiki/files?under=brand%2Fprint");
        assert_eq!(folder_href(""), "/wiki/files");
    }
}
