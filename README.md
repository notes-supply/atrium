# Atrium

Atrium is a private homepage and dashboard for one person, reached over a tailnet. Its first feature reads the wiki that Mneme holds, through Mneme's JSON reading API.

Pages are HTML that works without script, in the design the owner chose, which lives in `crates/atrium-ui`. Rust Wasm modules add behaviour on top, loaded by `data-module` attributes: the field behind every page, the command palette, keys, live search, link previews and the rest, in two bundles described in `web/README.md`.

## Run it

```sh
mise run build:wasm
cargo run -- --viewer tailnet --mneme-url http://127.0.0.1:8081 --addr 127.0.0.1:8080
```

`mise run dev` does both, with `ATRIUM_MNEME_URL` or `http://127.0.0.1:8081` as Mneme. `mise run demo` serves the design's invented wiki with no Mneme (`--demo`, or `--demo away` for the wiki out of reach). Every flag can also be set in the environment:

| Flag | Environment | Meaning |
| --- | --- | --- |
| `--addr` | `ATRIUM_ADDR` | Where to listen. `0.0.0.0:8080` by default. |
| `--viewer` | `ATRIUM_VIEWER` | Who reaches this listener: `tailnet` sees private features, `anonymous` only public ones. Required. |
| `--allowed-hosts` | `ATRIUM_ALLOWED_HOSTS` | Host names requests may name, comma-separated. `localhost` by default. `/healthz` answers any. |
| `--mneme-url` | `ATRIUM_MNEME_URL` | Mneme's base URL. Required. |
| `--mneme-timeout-ms` | `ATRIUM_MNEME_TIMEOUT_MS` | How long one call to Mneme may take. 3000 by default. Home waits at most 500 ms. |
| `--time-zone` | `ATRIUM_TIME_ZONE` | The reader's time zone, for every time shown and the light of the hour. The system's by default, from `TZ` or `/etc/localtime`, or UTC when it names none. The demo's is `Europe/Stockholm`. |
| `--demo` | | Serve the invented wiki instead of a Mneme, `--mneme-url` not needed. |

Logs are JSON lines on standard error, one per request. SIGTERM stops it after the requests in flight.

## How it is built

| Path | Holds |
| --- | --- |
| `crates/atrium` | The server: configuration, the registry (`src/features.rs`), middleware, the visit cookie, assets, and home. |
| `crates/atrium-core` | The contract: `Feature`, `Access`, `Viewer`, `Tile`, `Command`. |
| `crates/atrium-ui` | The design: the page shell, components, drawings, stylesheet and fonts. |
| `crates/atrium-wiki` | The wiki: Mneme's client, one module per page, Markdown rendering, and stand-ins for Mneme for tests and the demo. |
| `web/ui`, `web/field` | The browser modules in two Wasm bundles, every small behaviour and the WebGL field, with their browser tests. `web/loader.js` is the only script a page loads. |
| `tools/browser.sh` | Runs a command in Playwright's image against `atrium --demo`, with the npm packages `package-lock.json` pins. |
| `tools/check` | The parity check against the prototype. |

A feature implements `atrium_core::Feature`: an id, a name, its routes, and optionally its access, navigation link, home tiles and palette commands. Access defaults to `Private`, which only the tailnet sees. The server builds its router, navigation, home page and palette from the registry alone, and one middleware refuses a route to a viewer its feature does not admit. So a new feature is one module in `crates/atrium/src/features/` and one line in `registry()`. A feature with dependencies of its own gets a crate, as the wiki does.

`docs/decisions.md` records the choices that were not obvious.

## Deploy

A push to `main` publishes a release once the gate passes on that commit. CI's release job pushes the image built by the `image` job to `ghcr.io/notes-supply/atrium`, tagged with the commit, and `tools/release.sh` pins that digest in `deploy/`, pushes `deploy/` as one OCI artifact to `ghcr.io/notes-supply/manifests/atrium`, signs the image and the artifact keyless as `.github/workflows/release.yml` on `main`, and points the artifact's `main` tag at it.

`deploy/` is a Kustomize base that holds only what a release may change: two pods spread over nodes, rolled one at a time, and their disruption budget. They run as UID 65532 on a read-only root, listen on port 8080, read Mneme at `http://mneme:8080` and serve the owner. A cluster running Flux can follow the artifact's `main` tag and apply only what cosign verifies, with the issuer `https://token.actions.githubusercontent.com` and this identity:

```
^https://github\.com/notes-supply/atrium/\.github/workflows/release\.yml@refs/heads/main$
```

Such a cluster adds its own specifics as an overlay: the host name in `ATRIUM_ALLOWED_HOSTS`, the time zone in `ATRIUM_TIME_ZONE`, and the Services, Ingress, network policy and secrets around the pods. The project's own cluster does that in its own repository. It also checks every pod a release creates in namespace `atrium` and refuses it if it:

- runs as a service account other than `default`;
- mounts a service account token (keep `automountServiceAccountToken: false`);
- mounts an inline CSI volume;
- reads any Secret: Atrium needs none, and the one a pod there may read is Mneme's `mneme-tokens`;
- names a pull secret;
- carries the label `app.kubernetes.io/created-by=volsync`, which only the backup's Jobs may wear.

A refused pod is never created, so that release never gets ready, as below.

Roll back a bad release with a revert commit on `main`; it publishes the same way.
A release whose pods never get ready leaves the old pods serving, and Flux applies the revert once that release's health check has timed out, after 5 minutes in the project's cluster.
Never retag the artifact by hand: Flux accepts only what the release workflow signed on `main`.

## Check it

```sh
mise run gate
```

The gate builds the Wasm bundles and holds them to their size budgets, checks formatting, runs clippy natively and for Wasm with warnings denied, runs the tests with nextest against a stand-in that serves Mneme's example responses, checks the RustSec advisories and lints the CI workflow. Set `MISE_OFFLINE=1` to check the advisories against the local copy of the database.

`mise run test:mneme` runs one more test against a real `mneme serve`, with a wiki pushed by `git`. Set `ATRIUM_MNEME_BIN` to a `mneme` binary.

`mise run test:browser` drives every browser module, every route at 1440 and 390 px in day and night, and axe with scripts on and off, in Chromium against `atrium --demo`. `mise run axe` runs only axe. CI runs the browser tests in a job of their own, and `gate` passes only when both jobs did.

`mise run parity` screenshots every route of the demo beside the prototype page it stands for, at 1440 and 390 px in day and night, and reports the share of differing pixels: scripts off, or with `-- --scripts`, scripts on under reduced motion. It writes to `target/check`. `PROTOTYPE` must name the prototype's folder.

`mise run capture` records a short visit to the demo and screenshots home, a page and the machines at both widths in day and night, into `target/check/final`.

These run in Playwright's image through `tools/browser.sh`, which needs Docker and three free ports from `PORT` (18760 by default), and installs Playwright 1.64.1 and axe from `package-lock.json` with `npm ci` in the image.

`mise run image` builds the container image, answers home and serves the bundles with it, and scans it with Trivy when Trivy is installed. Nothing is pushed.

## License

Atrium is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Atrium by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
