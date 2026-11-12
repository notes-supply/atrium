# Contributing

Thank you for helping. [docs/development.md](docs/development.md) says how Atrium is put together and how to run it from a checkout.

- **Check your change** with `mise run gate`, which CI runs too. Run `mise run test:browser` as well when you change a page or a browser module; it needs Docker.
- **Write each commit message** as one imperative sentence with no prefix, such as "Show the newest change first".
- **Open pull requests against `main`.** Every commit on `main` deploys once its checks pass, so keep `main` green.
- **Record a choice that was not obvious** in [docs/decisions.md](docs/decisions.md).

Report security problems privately, as [SECURITY.md](SECURITY.md) says, not in an issue.
