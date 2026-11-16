//! Numbers and counts as the pages write them.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

const SMALL: [&str; 11] = [
    "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// `three pages`, `one page`, `no pages`, `12 pages`.
#[must_use]
pub fn count(n: u64, one: &str, many: &str) -> String {
    let number = usize::try_from(n)
        .ok()
        .and_then(|i| SMALL.get(i))
        .map_or_else(|| n.to_string(), |word| (*word).to_owned());
    format!("{number} {}", if n == 1 { one } else { many })
}

/// The text with its first letter in capitals.
#[must_use]
pub fn cap(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// `3,118`.
#[must_use]
pub fn number(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A size in bytes, in binary units as Mneme states its limits: `62 B`, `2.9 KiB`, `12 KiB`,
/// `100 MiB`. One decimal below ten, none above.
#[must_use]
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a size shown to one decimal place"
    )]
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let shown = if value < 9.95 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    };
    format!("{shown} {}", UNITS[unit])
}

/// Text as a query string value.
#[must_use]
pub fn query(text: &str) -> String {
    utf8_percent_encode(text, NON_ALPHANUMERIC).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_as_words_up_to_ten() {
        assert_eq!(count(0, "page", "pages"), "no pages");
        assert_eq!(count(1, "page", "pages"), "one page");
        assert_eq!(count(5, "new", "new"), "five new");
        assert_eq!(count(12, "page", "pages"), "12 pages");
        assert_eq!(cap("three questions"), "Three questions");
    }

    #[test]
    fn numbers_group_thousands() {
        assert_eq!(number(64), "64");
        assert_eq!(number(3118), "3,118");
        assert_eq!(number(1_234_567), "1,234,567");
    }

    #[test]
    fn sizes_read_in_binary_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(62), "62 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(3000), "2.9 KiB");
        assert_eq!(bytes(12_000), "12 KiB");
        assert_eq!(bytes(104_857_600), "100 MiB");
        assert_eq!(bytes(10 * 1024 * 1024 - 1), "10 MiB");
    }
}
