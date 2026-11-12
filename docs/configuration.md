# Configuration

Atrium takes flags, and every flag can also be set in the environment:

| Flag | Environment | Meaning |
| --- | --- | --- |
| `--addr` | `ATRIUM_ADDR` | Where to listen. `127.0.0.1:8080` by default; the image sets `0.0.0.0:8080`. |
| `--viewer` | `ATRIUM_VIEWER` | Who reaches this listener: a `member`, such as anyone on the team's tailnet, sees private features, and `anonymous` only public ones. `tailnet` and `owner` are other names for `member`. Required, but `member` by default with `--wiki-folder`. |
| `--allowed-hosts` | `ATRIUM_ALLOWED_HOSTS` | Host names requests may name, comma-separated, each with any port. `localhost,127.0.0.1,[::1]` by default. `/healthz` answers any. |
| `--mneme-url` | `ATRIUM_MNEME_URL` | Mneme's base URL. Required without `--wiki-folder` or `--demo`. |
| `--wiki-folder` | `ATRIUM_WIKI_FOLDER` | A wiki folder on this machine, read through a `mneme serve --folder` that Atrium starts. Needs a loopback `--addr`, and makes `member` the default viewer. |
| `--mneme-bin` | `ATRIUM_MNEME_BIN` | The Mneme that `--wiki-folder` starts. `mneme` on `PATH` by default. |
| `--record-name` | `ATRIUM_RECORD_NAME` | What the footer, the map and the machines page call the place that holds the wiki's record. The host of `--mneme-url` by default, this machine's short host name with `--wiki-folder`, and `laurel` in the demo. |
| `--mneme-timeout-ms` | `ATRIUM_MNEME_TIMEOUT_MS` | How long one call to Mneme may take. 3000 by default. Home waits at most 500 ms. |
| `--time-zone` | `ATRIUM_TIME_ZONE` | The reader's time zone, for every time shown and the light of the hour. The system's by default, from `TZ` or `/etc/localtime`, or UTC when it names none. The demo's is `Europe/Stockholm`. |
| `--demo` | | Serve the invented wiki instead of a Mneme, `--mneme-url` not needed. `--demo away` shows it out of reach. |

Every page shows the whole wiki to every member, and there is no login: whoever reaches a `member` listener reads everything. So Atrium listens on loopback unless `--addr` says otherwise, refuses any other address with `--wiki-folder`, and answers only the host names in `--allowed-hosts`. A proxy in front of it, such as `tailscale serve`, decides who reaches it ([install.md](install.md#share-it-on-a-tailnet)).

## Logs

Logs are JSON lines on standard error: one when Atrium listens, naming its address and viewer, one per request, and one when it stops. The lines of the Mneme that `--wiki-folder` starts are among them. Atrium stops with an error when that Mneme does, so that a supervisor starts both again. SIGTERM stops it after the requests in flight.
