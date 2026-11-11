//! The stylesheet, the fonts and the dither masks the stylesheet names. The server serves each
//! under a name with its hash in it, so the stylesheet is put together with the URLs it gets.

use std::fmt::Write;

/// One file the stylesheet refers to.
#[derive(Debug, Clone, Copy)]
pub struct File {
    /// The file's name without its extension, as the stylesheet names it.
    pub stem: &'static str,
    pub extension: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! files {
    ($folder:literal, $extension:literal, [$($stem:literal),* $(,)?]) => {
        [$(File {
            stem: $stem,
            extension: $extension,
            bytes: include_bytes!(concat!("../assets/", $folder, "/", $stem, ".", $extension)),
        }),*]
    };
}

/// The five subset fonts.
pub const FONTS: [File; 5] = files!(
    "fonts",
    "woff2",
    [
        "newsreader-400",
        "newsreader-600",
        "newsreader-italic-400",
        "plexmono-400",
        "plexmono-500",
    ]
);

/// The fonts the first paint needs, which every page preloads.
pub const PRELOADED: [&str; 2] = ["newsreader-400", "plexmono-400"];

/// The ordered-dither masks, at 0 to 16 sixteenths in steps of two.
pub const DITHER: [File; 9] = files!(
    "dither",
    "svg",
    ["d0", "d2", "d4", "d6", "d8", "d10", "d12", "d14", "d16"]
);

/// Every colour, size, space and motion value.
const TOKENS: &str = include_str!("../assets/tokens.css");
/// The rules, which import the tokens and the drawn sizes.
const RULES: &str = include_str!("../assets/atrium.css");
/// What the server adds to the rules.
const PORT: &str = include_str!("../assets/port.css");

/// Sizes that follow from the drawings and the data: the map's proportions, and a class per
/// percent for the reading progress bar, since a page may not carry `style` attributes.
fn drawn() -> String {
    let (width, height) = crate::draw::map_size();
    let mut css = format!(".map-stage {{ aspect-ratio: {width} / {height}; }}\n");
    for percent in 0..=100 {
        let _ = writeln!(
            css,
            ".pb-fill.pct-{percent} {{ width: {percent}%; }} .pb-n.pct-{percent} {{ left: {percent}%; }}"
        );
    }
    css
}

/// The one stylesheet the pages link: the tokens, the drawn sizes and the rules, in the order
/// the rules import them, with each font and mask at the URL `url_of` gives it.
#[must_use]
pub fn stylesheet(url_of: impl Fn(&File) -> String) -> String {
    let rules: String = RULES
        .lines()
        .filter(|line| !line.starts_with("@import "))
        .flat_map(|line| [line, "\n"])
        .collect();
    let mut css = format!("{TOKENS}\n{}\n{rules}\n{PORT}", drawn());
    for (folder, files) in [("../fonts/", &FONTS[..]), ("dither/", &DITHER[..])] {
        for file in files {
            let written = format!("url(\"{folder}{}.{}\")", file.stem, file.extension);
            css = css.replace(&written, &format!("url(\"{}\")", url_of(file)));
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css() -> String {
        stylesheet(|file| format!("/assets/{}.hash.{}", file.stem, file.extension))
    }

    #[test]
    fn every_url_in_the_stylesheet_is_an_asset() {
        let css = css();
        let urls: Vec<&str> = css.split("url(").skip(1).collect();
        assert_eq!(urls.len(), FONTS.len() + DITHER.len());
        for url in urls {
            assert!(
                url.starts_with("\"/assets/"),
                "{}",
                &url[..40.min(url.len())]
            );
        }
    }

    #[test]
    fn the_stylesheet_imports_nothing_and_holds_the_tokens_first() {
        let css = css();
        assert!(!css.contains("@import"));
        assert!(css.starts_with("/* Atrium tokens."));
        assert!(css.contains(".pb-fill.pct-62 { width: 62%; }"));
    }
}
