# Atrium

Atrium is a private homepage and dashboard for one person, reached over a tailnet. Its first feature reads the wiki that Mneme holds, through Mneme's JSON reading API.

Pages are plain HTML that works without script. A Rust Wasm module adds a command palette on Ctrl+K or Cmd+K. The look is a placeholder until a design is chosen; it lives in `crates/atrium-ui`.

## Run it

```sh
mise run build:wasm
cargo run -- --viewer tailnet --mneme-url http://127.0.0.1:8081 --addr 127.0.0.1:8080
```

`mise run dev` does both, with `ATRIUM_MNEME_URL` or `http://127.0.0.1:8081` as Mneme. Every flag can also be set in the environment:

| Flag | Environment | Meaning |
| --- | --- | --- |
| `--addr` | `ATRIUM_ADDR` | Where to listen. `0.0.0.0:8080` by default. |
| `--viewer` | `ATRIUM_VIEWER` | Who reaches this listener: `tailnet` sees private features, `anonymous` only public ones. Required. |
| `--allowed-hosts` | `ATRIUM_ALLOWED_HOSTS` | Host names requests may name, comma-separated. `localhost` by default. `/healthz` answers any. |
| `--mneme-url` | `ATRIUM_MNEME_URL` | Mneme's base URL. Required. |
| `--mneme-timeout-ms` | `ATRIUM_MNEME_TIMEOUT_MS` | How long one call to Mneme may take. 3000 by default. |

Logs are JSON lines on standard error, one per request. SIGTERM stops it after the requests in flight.

## How it is built

| Path | Holds |
| --- | --- |
| `crates/atrium` | The server: configuration, the registry (`src/features.rs`), middleware, assets, and the small features `home` and `clock`. |
| `crates/atrium-core` | The contract: `Feature`, `Access`, `Viewer`, `Tile`, `Command`. |
| `crates/atrium-ui` | The page shell, shared components and the stylesheet. The chosen design replaces most of it. |
| `crates/atrium-wiki` | The wiki: Mneme's client, the routes, Markdown rendering, and a stand-in for Mneme for tests. |
| `web/palette` | The command palette, a Wasm module. `web/loader.js` is the only script a page loads. |

A feature implements `atrium_core::Feature`: an id, a name, its routes, and optionally its access, navigation link, home tiles and palette commands. Access defaults to `Private`, which only the tailnet sees. The server builds its router, navigation, home page and palette from the registry alone, and one middleware refuses a route to a viewer its feature does not admit. So a new feature is one module in `crates/atrium/src/features/` and one line in `registry()`, as `clock.rs` shows. A feature with dependencies of its own gets a crate, as the wiki does.

`docs/decisions.md` records the choices that were not obvious.

## Check it

```sh
mise run gate
```

The gate builds the Wasm module, checks formatting, runs clippy natively and for Wasm with warnings denied, runs the tests with nextest against a stand-in that serves Mneme's example responses, checks the RustSec advisories and lints the CI workflow. Set `MISE_OFFLINE=1` to check the advisories against the local copy of the database.

`mise run test:mneme` runs one more test against a real `mneme serve`, with a wiki pushed by `git`. Set `ATRIUM_MNEME_BIN` to a `mneme` binary.

`mise run image` builds the container image, answers a request with it, and scans it with Trivy when Trivy is installed. Nothing is pushed.
