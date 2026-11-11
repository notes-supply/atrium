# Atrium

Atrium is a private homepage and dashboard for one person, reached over a tailnet.

## Check it

```sh
mise run gate
```

The gate checks formatting, runs clippy with warnings denied, runs the tests with nextest, checks the RustSec advisories and lints the CI workflow. Set `MISE_OFFLINE=1` to check the advisories against the local copy of the database without fetching it.
