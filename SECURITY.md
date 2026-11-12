# Security

## Supported versions

The latest release is supported. Fixes go into a new release; older ones get none.

## Reporting a vulnerability

Report a vulnerability privately through GitHub: choose "Report a vulnerability" on the repository's [Security tab](https://github.com/notes-supply/atrium/security). Please don't open a public issue for it.

Atrium has no login: whoever reaches a listener for members reads the whole wiki, by design. Who reaches it is the network's or the proxy's to decide ([docs/configuration.md](docs/configuration.md)). A way past `--allowed-hosts`, the access check, the content security policy or loopback-only `--wiki-folder` is a vulnerability.
