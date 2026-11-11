# Decisions

Choices made while building Atrium that the skeleton brief left open or that differ from it.

## Toolchain and gate

- **Nightly 2026-10-28,** as in Mneme. It is the newest dated nightly already installed on the build machine with the `wasm32-unknown-unknown` target and `rust-src`. Move the date in `mise.toml` deliberately.
- **The lint table, `deny.toml`, `rustfmt.toml`, `clippy.toml` and the CI workflow are Mneme's.** The lints live in `[workspace.lints]` so every crate shares them.
- **`mise.lock` was written from Mneme's lock.** The tools are the same versions, and `mise lock` cannot verify their provenance offline.
- **The integration tests carry one `expect(clippy::unwrap_used)`.** Clippy flags `unwrap` in helpers outside test functions here, though it does not in Mneme's identical layout. The cause was not found.

## Crates and the registry

- **`atrium-core` holds the contract:** `Feature`, `Access`, `Viewer`, `Tile`, `Command` and `NavEntry`. It is apart from `atrium-ui` so that replacing the design does not touch what features implement.
- **Home lives in the server crate,** beside the registry it reads, and small features such as the clock do too. A feature with its own dependencies, such as the wiki, is a crate. The brief suggested an `atrium-home` crate; it would have needed the registry handed back to it.
- **`Feature::access` defaults to `Private`,** so a feature that says nothing is denied to everyone but the tailnet.
- **Every feature's routes are mounted for every listener,** and one middleware refuses those the listener's viewer may not see. Navigation, commands and tiles leave such features out.

## Requests and responses

- **A refused viewer gets the same 404 page as an unknown address,** so a public listener would not tell which private routes exist.
- **A request naming a host not in `--allowed-hosts` gets 421.** `/healthz` answers any host, since a kubelet probe names the pod's address.
- **`/assets/*` sits outside the features** and answers every viewer. It holds the stylesheet and the palette, which carry nothing private, and a public listener will need the same files. The brief allowed only health checks outside a feature.
- **The CSP adds `connect-src 'self'`** to the brief's list. With `default-src 'none'` the module loader could not fetch its `.wasm` without it.
- **Every page's `ETag` is a hash of the rendered body,** not Mneme's. A page holds Atrium's own markup and navigation as well as Mneme's data, so Mneme's tag would stay the same across an Atrium release that changes the page. Pages are also `no-store`, as the brief asks, so browsers rarely send `If-None-Match`; other clients can.

## The wiki

- **Times are shown in UTC,** and recent changes are grouped by UTC day. Atrium has no time-zone setting yet; the distroless image carries the zone database for when it does.
- **A link written as a file path,** such as `/wiki/notes/deep.md`, is looked up among the live pages, 500 at a time, and answered with 303 to the page's id. Mneme has no lookup by path. A path a page has since been renamed away from is not found.
- **A link whose scheme is not `http`, `https` or `mailto` becomes its text.** comrak alone would keep it as a link with an empty `href`. Relative links to other files, such as images, stay as written; Mneme serves no files.
- **Headings keep comrak's ids and its permalink anchor,** which has an `aria-label`, so `page.md#section` links land. The skip link's target is `main-content`, which only a heading named "Main content" would share.
- **A page over Mneme's markup budget is shown as its raw text,** as Mneme indexes it, so one dense page cannot make comrak hold a very large tree.
- **A diff line carries its sign as text** (`-`, `+`), inside `<del>` or `<ins>`, with changed words in `<mark>`. Screen readers do not announce `<del>` and `<ins>`.
- **Mneme errors map to one response each:** away, slow or failing is 503 with `Retry-After: 30`; an unknown page or commit is 404; a search Mneme cannot read is 400 with Mneme's reason; an answer Atrium cannot parse is 502.
