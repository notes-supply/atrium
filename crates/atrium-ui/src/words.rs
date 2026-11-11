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
}
