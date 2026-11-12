# Mneme and Atrium on a small server

`compose.yaml` runs Mneme, which holds the wiki's record and takes the writers' pushes, and Atrium in front of it, on one small always-on machine, with `podman compose` or `docker compose`. Both run as UID 65532 on a read-only root with every capability dropped, as `deploy/` runs them, and both are published on 127.0.0.1 only: Mneme on port 8081 and Atrium on port 8080.

## Pick the images

CI publishes both images for every commit of main once its checks pass, tagged with the commit, for x86_64. Name the commits in a `.env` file beside `compose.yaml`, such as the newest of each:

```sh
cat >.env <<EOF
MNEME_COMMIT=$(git ls-remote https://github.com/notes-supply/mneme refs/heads/main | cut -f1)
ATRIUM_COMMIT=$(git ls-remote https://github.com/notes-supply/atrium refs/heads/main | cut -f1)
ATRIUM_TIME_ZONE=Europe/Stockholm
ATRIUM_RECORD_NAME=$(hostname -s)
EOF
```

`ATRIUM_TIME_ZONE` is the zone the pages show times in, UTC when it is left out. `ATRIUM_RECORD_NAME` is what Atrium's footer, map and machines page call this server, which holds the record; `mneme`, the host Atrium reaches it at, when it is left out.

## Make the tokens

Every writing machine has a token of its own, and Mneme keeps only its SHA-256, in `tokens.json`, under the machine's name. For a machine named `laptop`:

```sh
TOKEN=$(head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n')
printf '{ "laptop": "%s" }' "$(printf %s "$TOKEN" | sha256sum | cut -d' ' -f1)" >tokens.json
chmod 644 tokens.json
```

Give `$TOKEN` itself to that machine, in `~/.config/mneme/token` with mode 0600, and keep no other copy. More machines are more entries in the same object. The file holds only hashes, so the container's user may read it. Mneme reads it at start, so run `podman compose restart mneme` after changing it.

## Start them

```sh
podman compose up -d
```

`http://localhost:8080` is Atrium on this machine. The wiki's repository lives in the volume `mneme`, which is the one thing to back up.

## Put them on a tailnet

`tailscale serve` on this machine gives both an HTTPS name on the tailnet, `https://<machine>.<tailnet>.ts.net`. Serve Atrium at `/` and only Mneme's Git routes at `/git`:

```sh
tailscale serve --bg 8080
tailscale serve --bg --set-path /git http://127.0.0.1:8081/git
```

`tailscale serve` takes the mount path off a request before it proxies it and adds the target's own path back, so `/git/wiki.git` reaches Mneme as `/git/wiki.git`. This is how its source behaves (`ipn/ipnlocal/serve.go`, which strips the mount point, and `ipn/serve.go`, which keeps the target's path); its documentation does not say. It also keeps the request's `Host`, which Atrium checks, so add the name to `.env` and start Atrium again:

```sh
echo 'ATRIUM_ALLOWED_HOSTS=localhost,<machine>.<tailnet>.ts.net' >>.env
podman compose up -d
```

Mneme's reading API, `/v1`, has no login, so it stays on this machine, where only Atrium reads it. Atrium's pages show the whole wiki to everyone the tailnet lets reach this machine, so check its access rules first. Never use `tailscale funnel` for either: that would put them on the public internet.

## Connect the writers

On each writing machine, with its token in `~/.config/mneme/token` and its short host name as the name in `tokens.json`:

```sh
mneme init --server https://<machine>.<tailnet>.ts.net --token-file ~/.config/mneme/token
```

`mneme init` makes `~/wiki` ready and starts Mneme's autosave service, which commits, pulls and pushes the folder on its own. Mneme's README says more.
