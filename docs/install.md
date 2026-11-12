# Install and run

## The installer

Mneme's installer installs the latest `mneme` and `atrium` into `~/.local/bin`, on Linux on x86_64 or aarch64:

```sh
curl -fsSL https://raw.githubusercontent.com/notes-supply/mneme/main/install.sh | sh
```

It checks each binary against its release's `SHA256SUMS`, and against its signature when `cosign` is on `PATH`, before it moves it into place. It also puts Atrium's systemd user unit, `contrib/atrium.service`, in `~/.config/systemd/user`, without enabling it, unless one is there already. `sh -s -- atrium` installs Atrium alone, and `MNEME_INSTALL_DIR` names another folder.

## A wiki folder

`mneme init` makes `~/wiki` a clone of the wiki and keeps it saved and in sync. The wiki lives on a Git host over HTTPS with a token, on a Mneme server, or in a repository on this machine:

```sh
mneme init --remote https://github.com/your-team/wiki.git --token-file ~/.config/mneme/token
mneme init --server https://mneme.example --token-file ~/.config/mneme/token
mneme init --remote ~/wiki.git
```

Mneme's README says more about each.

## Run it

```sh
atrium --wiki-folder ~/wiki
```

serves the folder on http://localhost:8080. Atrium starts `mneme serve --folder ~/wiki` itself and stops it when it stops, and a new commit shows within seconds. To keep it running, enable the unit the installer put in place; its logs are in `journalctl --user -u atrium`:

```sh
systemctl --user enable --now atrium
```

`atrium --demo --viewer member` serves an invented wiki instead, with no Mneme, which is what the screenshots show.

## Share it on a tailnet

Every page shows the whole wiki and there is no login, so with `--wiki-folder` Atrium listens on loopback only, `127.0.0.1:8080` by default, and refuses any other `--addr`. `tailscale serve` on the same machine keeps it there and adds HTTPS. Allow the name it is reached by:

```sh
tailscale serve --bg 8080
atrium --wiki-folder ~/wiki --allowed-hosts localhost,<machine>.<tailnet>.ts.net
```

Then open `https://<machine>.<tailnet>.ts.net`. For the unit, uncomment its `Environment=ATRIUM_ALLOWED_HOSTS=` line with that name, then run `systemctl --user daemon-reload` and `systemctl --user restart atrium`.

Everyone your tailnet lets reach this machine can then read the whole wiki, so check its access rules first. Never use `tailscale funnel` for it: that would put the wiki on the public internet.

For a team, [`examples/server`](../examples/server) runs Mneme and Atrium on one small always-on machine, and puts them on a tailnet the same way.

## Verify a download

Each release has a static `atrium` for Linux on x86_64 and aarch64, `SHA256SUMS`, and a cosign bundle per binary, signed keyless by `.github/workflows/binaries.yml` on the release's tag. To check one with [cosign](https://docs.sigstore.dev/cosign/system_config/installation/) and install it only if it passes, with `version` the release you want:

```sh
version=vX.Y.Z
file=atrium-$(uname -m)-linux
base=https://github.com/notes-supply/atrium/releases/download/$version
curl -fsSL -O "$base/$file" -O "$base/$file.sigstore.json" &&
  cosign verify-blob --bundle "$file.sigstore.json" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    --certificate-identity "https://github.com/notes-supply/atrium/.github/workflows/binaries.yml@refs/tags/$version" \
    "$file" &&
  install -D -m 755 "$file" ~/.local/bin/atrium
```

The unit is `contrib/atrium.service` at the same tag; copy it to `~/.config/systemd/user/` and run `systemctl --user daemon-reload`. To build Atrium from a checkout instead, see [development.md](development.md).
