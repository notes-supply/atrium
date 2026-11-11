# Decisions

Choices made while building Atrium that the skeleton brief left open or that differ from it.

## Toolchain and gate

- **Nightly 2026-10-28,** as in Mneme. It is the newest dated nightly already installed on the build machine with the `wasm32-unknown-unknown` target and `rust-src`. Move the date in `mise.toml` deliberately.
- **The lint table, `deny.toml`, `rustfmt.toml`, `clippy.toml` and the CI workflow are Mneme's.** The lints live in `[workspace.lints]` so every crate shares them.
- **`mise.lock` was written from Mneme's lock.** The tools are the same versions, and `mise lock` cannot verify their provenance offline.
