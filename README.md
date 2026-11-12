# Atrium

[![CI](https://github.com/notes-supply/atrium/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/notes-supply/atrium/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/notes-supply/atrium)](https://github.com/notes-supply/atrium/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

Atrium is a shared homepage and dashboard for a team. Its first feature is the wiki that [Mneme](https://github.com/notes-supply/mneme) keeps in Git: pages, history, diffs, recent changes, search, and the state of every machine that writes to it. When many people and agents work on one wiki, everyone on the network sees the same view of what changed, who changed it and what is stuck, within seconds of each save.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/home-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="docs/images/home-light.png">
  <img alt="Atrium's home: a sentence on what changed since the last visit, a map of the machines that write to the wiki with one blocked, and the recent changes" src="docs/images/home-light.png">
</picture>

## Features

- **What changed since your last visit,** in one sentence on home, with the questions the wiki leaves open.
- **Every change, by whom:** recent changes by day, each page's history, and diffs by paragraph or by line.
- **Edits that overlapped,** when two machines changed the same lines, marked and linked to both sides.
- **What is stuck:** a map of the machines that write to the wiki, and which are blocked, behind or leaving files out.
- **Find anything fast:** search as you type, a command palette, keys for every section, and link previews.
- **Light to run:** pages work without scripts, from one static binary on a laptop or a 13 MB image on a server.

## Getting started

Install `mneme` and `atrium` into `~/.local/bin`, checked against their releases, with Atrium's systemd user unit:

```sh
curl -fsSL https://raw.githubusercontent.com/notes-supply/mneme/main/install.sh | sh
```

Clone the team's wiki into `~/wiki`, which Mneme keeps saved and in sync, then start Atrium in front of it:

```sh
mneme init --remote https://github.com/your-team/wiki.git --token-file ~/.config/mneme/token
systemctl --user enable --now atrium
```

Open http://localhost:8080. To look around first, `atrium --demo --viewer member` serves the invented wiki in the screenshot.

Atrium has no login and anyone who reaches it reads the whole wiki, so it listens on loopback; [share it with `tailscale serve`](docs/install.md#share-it-on-a-tailnet), never `tailscale funnel`.

For the whole team, [`examples/server`](examples/server) runs Mneme and Atrium on one small server. Other remotes and setups are in [Install and run](docs/install.md).

## Documentation

- [Install and run](docs/install.md): the installer, other remotes, sharing on a tailnet, and verifying a download.
- [Configuration](docs/configuration.md): every flag and environment variable, and the logs.
- [Server example](examples/server): Mneme and Atrium with compose.
- [Deployment](docs/deploy.md): the image, releases, and a cluster running Flux.
- [Development](docs/development.md): building, the gate, the browser tests and the screenshots.
- [Browser modules](web/README.md) and [decisions](docs/decisions.md).
- [Screenshots](docs/screenshots.md): a page, a diff and the machines.

## Contributing

Contributions are welcome; [CONTRIBUTING.md](CONTRIBUTING.md) says how.

## Security

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md) describes.

## License

Atrium is licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Atrium by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
