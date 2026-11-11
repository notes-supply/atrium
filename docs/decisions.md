# Decisions

Choices made while building Atrium that the skeleton brief left open or that differ from it.

## Toolchain and gate

- **Nightly 2026-10-28,** as in Mneme. It is the newest dated nightly already installed on the build machine with the `wasm32-unknown-unknown` target and `rust-src`. Move the date in `mise.toml` deliberately.
- **The lint table, `deny.toml`, `rustfmt.toml`, `clippy.toml` and the CI workflow are Mneme's.** The lints live in `[workspace.lints]` so every crate shares them.
- **`mise.lock` was written from Mneme's lock.** The tools are the same versions, and `mise lock` cannot verify their provenance offline.
- **The integration tests carry one `expect(clippy::unwrap_used)`.** Clippy flags `unwrap` in helpers outside test functions here, though it does not in Mneme's identical layout. The cause was not found.

## Crates and the registry

- **`atrium-core` holds the contract:** `Feature`, `Access`, `Viewer`, `Tile`, `Command` and `NavEntry`. It is apart from `atrium-ui` so that replacing the design does not touch what features implement.
- **Home lives in the server crate,** beside the registry it reads. A feature with its own dependencies, such as the wiki, is a crate. The brief suggested an `atrium-home` crate; it would have needed the registry handed back to it. Home's layout is in `atrium-ui`, because the wiki's pages show the same layout when Mneme is away.
- **The clock feature is gone.** It proved the registry in the skeleton; the chosen design has no clock tile or page, and the masthead shows the time.
- **A feature offers navigation entries, a search target, a part of home and a footer line.** Home's part is clauses of the opening sentence, a drawing and tiles. The palette's commands are the navigation entries, which every page lists as plain links with keys `1` to `5`, each with the palette's longer name in `data-title` ("Recent changes" for "Changes"), so the routes and the names both come from the registry. The search target names the form's action and the page list the palette reads, `/wiki/index.json`.
- **`Feature::access` defaults to `Private`,** so a feature that says nothing is denied to everyone but the tailnet.
- **Every feature's routes are mounted for every listener,** and one middleware refuses those the listener's viewer may not see. Navigation, commands and tiles leave such features out.

## Requests and responses

- **A refused viewer gets the same 404 page as an unknown address,** so a public listener would not tell which private routes exist.
- **A request naming a host not in `--allowed-hosts` gets 421.** `/healthz` answers any host, since a kubelet probe names the pod's address.
- **`/assets/*` sits outside the features** and answers every viewer. It holds the stylesheet, the fonts, the loader and the bundles, which carry nothing private, and a public listener will need the same files. The brief allowed only health checks outside a feature.
- **The CSP adds `connect-src 'self'`** to the brief's list. With `default-src 'none'` the module loader could not fetch its `.wasm` without it.
- **Every page's `ETag` is a hash of the rendered body,** not Mneme's. A page holds Atrium's own markup and navigation as well as Mneme's data, so Mneme's tag would stay the same across an Atrium release that changes the page. Pages are also `no-store`, as the brief asks, so browsers rarely send `If-None-Match`; other clients can.

## The wiki

- **Times are shown in the reader's zone,** `--time-zone`, Stockholm by default, through `jiff` with its zone database built in, so the image needs none. `jiff` was already in the tree; `chrono-tz` would have added `chrono`. Recent changes are grouped by the reader's day, and `data-hour` on `<html>` is the reader's hour.
- **A link written as a file path,** such as `/wiki/notes/deep.md`, is asked of Mneme's `/v1/paths/{path}`, which answers the page there now or the one a rename moved away from it: 303 to its id. Mneme leaves deleted pages out, so a path it does not know is looked for among the deleted pages, 500 at a time, and a deleted one gets 404 with what the record remembers of it. That walk would go if `/v1/paths` also answered deleted pages, with their deletion time.
- **An overlap mark names the losing edit's author from `overlap_author`** and links its diff, since the losing edit is on the same page. An answer without the field resolves the commit in the page's history, as before.
- **Older changes are asked with `before`,** the start of the oldest day shown, so a page of days stays whole as new changes arrive. A Mneme that does not know `before` ignores it and answers from the newest, which Atrium sees in the answer and then pages by the `offset` the link also carries.
- **A machine is behind since Mneme's `behind_since`** when it sends one, and otherwise since the record's head time.
- **The pages' `questions` count is not shown.** The design has no place for a count per page; the Waiting for you tile reads `/v1/questions`.
- **A link whose scheme is not `http`, `https` or `mailto` becomes its text.** comrak alone would keep it as a link with an empty `href`. Relative links to other files, such as images, stay as written; Mneme serves no files.
- **Headings keep comrak's ids, without its permalink anchor,** so `page.md#section` links land and the keyboard meets no invisible link. A formatter writes the design's article: tables in a focusable region with cells named for the phone layout, code in a focusable figure, the "Open questions" list with its mark, and `data-preview` on links to other pages. The first level-1 heading is the title, which the page shows in its own header.
- **A page over Mneme's markup budget is shown as its raw text,** as Mneme indexes it, so one dense page cannot make comrak hold a very large tree.
- **A diff reads as paragraphs and as lines.** A removed line and the added line after it are one paragraph, the words both keep written once and the changed ones in `<del>` and `<ins>`; when the kept words do not line up, the old line is removed and the new one added whole. The line view carries each sign as text. Version numbers count every event, oldest first, so a rename has a number too.
- **Mneme errors map to one response each:** away, slow or failing is 503 with `Retry-After: 30`; an unknown page or commit is 404; a search Mneme cannot read is 400 with Mneme's reason; an answer Atrium cannot parse is 502.

## The Wasm pipeline

- **The pipeline is shebang-web's:** `build-std` with `optimize_for_size`, immediate abort, a `wasm` profile at `opt-level = "z"` with fat LTO, `wasm-bindgen --target web`, then `wasm-opt -Oz` with the features rustc enables. The crate is built as a `cdylib` only in `mise run build:wasm`, so native builds and tests treat it as a plain library.
- **wasm-bindgen is pinned at 0.2.129, one release behind crates.io's 0.2.130.** The CLI must equal the crate, and the build machine has a provenance-checked `mise.lock` entry for the 0.2.129 CLI and none for 0.2.130; writing a new one needs GitHub, which this build was not to use. Move both together.
- **The server embeds the built files** with `include_bytes!` from `target/web`, so `mise run build:wasm` comes before any Cargo build of the server. The gate's clippy and test tasks depend on it, and it is skipped when its sources have not changed.
- **Two bundles under hashed names.** The loader, `ui` and `field`, glue and `.wasm` each, are assets like the stylesheet, and the loader's own `<script>` names the other four. The output rules allow that one deferred script, at most 1 KB, with nothing inline.
- **The glue and the loader are not minified.** shebang-web minifies with Bun, which would be one more tool for about 1.5 KB of brotli. The loader is 739 bytes as written, 330 compressed.
- **Every page loads the loader,** since every page has modules (`keys`, `light`, `carry`, `field`, `prefs`, `palette`).
- **A link's preview is fetched, not written into the page.** The article names `/wiki/preview.json`; the module asks it for a link once the reader means it, after 320 ms of hover or on focus, keeps each answer for the page's life, and shows nothing when the fetch fails. A page with many links stays as small as one with none.
- **With scripts on, the visitor's clock wins** for the masthead clock, the greeting and the light of the hour. The server's `--time-zone` writes the page without scripts and the first paint; the two agree whenever the visitor is in that zone, so home does not change as the modules start.
- **The entrance is the server's choice.** `data-entrance` is `full` for a reader with no visit cookie or whose latest request was over eight hours ago, `short` otherwise. It is measured from the cookie's latest request, not from the previous visit, so the next page of a sitting comes in short, as the prototype's per-session flag had it. The demo keeps the cookie too.
- **The browser tests drive the real server.** They run against `atrium --demo` and the away demo, so the 908 KB of copied prototype pages are gone. The field's pixel check and its bench serve the prototype's own pages, with its scripts swapped for the loader at request time, when `PROTOTYPE` names it. A test names the requests it means to fail, such as the away wiki's page list, since Chromium reports each failed load in the console.
- **Playwright comes from the repository's own lock.** `package.json` pins `@playwright/test` and `playwright-core` 1.64.1, the image's version, and `@axe-core/playwright` 4.14.0, whose loose `playwright-core` range would otherwise pull a second, newer copy. `tools/browser.sh` installs them with `npm ci` inside Playwright's image when the lock changes.
- **CI runs the checks and the browser tests as two jobs, and `gate` is a third** that always runs and passes only when both did. A required job skipped because a need failed would count as passed.
- **Section keys are `1` to `5`,** the bracketed index in the masthead, as the design has them. The skeleton's `g w` sequences are gone with the footer's command list. A switch in the settings turns single-letter keys off (`atrium-keys=off`, `data-keys="off"` on `<html>`).

## The image

- **The image builds the server with Rust 1.99.1 stable** in `rust:1.99.1-alpine3.24`, not the gate's pinned nightly. The build was to fetch only crates and base images, and installing the nightly in the builder needs rustup's own servers. Nothing in the server needs nightly. The Wasm module is built by the pinned nightly on the host and copied in, so it is the same file either way.
- **The runtime is `distroless/static-debian13:nonroot`,** both bases pinned by digest. Debian 13 is the newest distroless static.
- **`mise run image` also runs the image:** read-only, it answers `/healthz` and home, and exits 0 on SIGTERM. It then fails on any High or Critical finding from Trivy when Trivy is installed. Trivy reads the image's Debian packages; the Rust dependencies are checked by `cargo deny` in the gate, since the binary carries no `cargo auditable` record.

## The chosen design

- **The prototype's files are copied as they are,** but for five rules. `tokens.css`, the fonts and the dither masks are byte for byte the prototype's, so a later change to the design is a diff against it. `atrium.css` differs only where the prototype wrote `html.js`: those rules ask `@media (scripting: enabled)` and read `html:root`, which keeps their specificity. The prototype's boot script set `js` before the first paint; the loader runs after parsing, too late for the entrance and for controls hidden without script. What the server adds is in `port.css`: the letter-key switch and a `[hidden]` rule.
- **One stylesheet.** The server folds `tokens.css`, the drawn sizes and `atrium.css` into one hashed file in the order the imports gave, so a first visit makes one request instead of three in a row. `tokens.css` stays the one source of every value.
- **The drawings are ported to Rust** (`atrium-ui/src/draw.rs`) and were compared byte for byte with the prototype's map, places and mark. A place is drawn by name: `blackbox` and `aeolus` have their own, another machine takes the first free one, and a third machine is on the machines page but not on the map. `data-m` names the drawing, which the stylesheet colours.
- **Pages work without scripts from the first paint.** The server writes `data-hour`, `data-theme`, `data-intensity` and `data-keys` from the clock and the cookies the settings write, so there is no boot script.
- **The last visit is a cookie the server keeps,** `atrium-visit`, holding the previous visit and this one. A pause over thirty minutes starts a new visit. It is `HttpOnly`, `SameSite=Lax`, and `Secure` when `X-Forwarded-Proto` says the request came over HTTPS.
- **Reading positions stay in the browser.** "Continue reading" and the "you were here before" note are hidden slots the reading module fills, so without scripts they are absent.
- **Machines are judged in one function** (`machines::judge`). Needs you: blocked, stopped, unseen for over ten minutes or never, or behind for over ten minutes, where behind is measured from the record's head time, the newest commit the machine has not reported. Worth a look: offline, or files kept out of saves. The legend states Mneme's meanings rather than the prototype's.
- **Home waits 500 ms** for its tiles, all fetched at once. A tile that misses it is a quiet "briefly away" tile and its clause leaves the sentence; when nothing answers, home takes the away look and still answers 200.
- **Lists page as the design shows them:** 16 pages, 10 events of a history, five days of changes.
- **The demo is the same code.** `atrium --demo` starts a stand-in for Mneme in the process, filled with the prototype's content in Mneme's shapes, and holds the clock at the prototype's 09:41. Its week is computed from its own changes, so it is sparser than the prototype's random one.

## Deployment

- **Flux on Apollo verifies cosign's classic `.sig` signature, so the release signs in that format.** Flux 2.8.2 found no signature in cosign 3.1.4's default bundle format, which a registry without the referrers API, GHCR among them, stores under a `sha256-<digest>` tag, and it verified the same artifact signed with `--new-bundle-format=false`. Shown on kind with a key; keyless signing is proven only once `release.yml` has run on GitHub.
- **The Services live in the cluster repository.** A release that may write a Service can claim an external IP on every node or ask the Tailscale operator for a new tailnet host, which changes the network beyond the namespace. The deployer may write only Deployments, StatefulSets, ConfigMaps and PodDisruptionBudgets, and read ReplicaSets and pods, which Flux's health check needs.
- **`release.yml` is a reusable workflow that CI calls after the gate.** The keyless identity Apollo verifies is that one file on `refs/heads/main`. The `image` job builds and checks the image with no publishing rights, and the release job pushes that exact archive.
- **A rerun of an older run publishes nothing.** The release job stops unless its commit is still `main`'s head, so only the newest commit can move the `main` tag. Runs on `main` are already serialised by the workflow's concurrency group, and the release job has its own.
- **Each release spreads its own pods** (`matchLabelKeys: [pod-template-hash]`). Counting the old ReplicaSet's pods mid-rollout put both new pods on one node in the kind rehearsal.
