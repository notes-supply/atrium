//! The size budgets for what a page downloads, brotli-compressed, against the files
//! `mise run build:wasm` leaves in `target/web`.

use std::io;
use std::path::Path;

const KB: usize = 1024;

fn compressed(paths: &[&str]) -> io::Result<usize> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 22,
        ..Default::default()
    };
    let mut total = 0;
    for path in paths {
        let bytes = std::fs::read(root.join(path))?;
        let mut out = Vec::new();
        brotli::BrotliCompress(&mut bytes.as_slice(), &mut out, &params)?;
        total += out.len();
    }
    Ok(total)
}

#[test]
fn the_ui_bundle_is_at_most_45_kb() {
    let size = compressed(&["target/web/ui_bg.wasm", "target/web/ui.js"]).unwrap();
    assert!(size <= 45 * KB, "{size} bytes");
}

#[test]
fn the_field_bundle_is_at_most_30_kb() {
    let size = compressed(&["target/web/field_bg.wasm", "target/web/field.js"]).unwrap();
    assert!(size <= 30 * KB, "{size} bytes");
}

#[test]
fn the_loader_is_at_most_1_kb() {
    let size = compressed(&["web/loader.js"]).unwrap();
    assert!(size <= KB, "{size} bytes");
}
