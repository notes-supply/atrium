#![expect(
    clippy::unwrap_used,
    reason = "helpers outside test functions fail the test by panicking"
)]

mod access;
mod html;
mod local;
mod output;
mod process;
mod real;
mod settings;
mod support;
mod wiki;
