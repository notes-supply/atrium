# Deploy

## The image

`Containerfile` builds a static `atrium` into `distroless/static-debian13:nonroot`, with no shell or package manager, as UID 65532. Atrium writes no files, so the root can be read-only. The image sets `ATRIUM_ADDR=0.0.0.0:8080`, since a container's port is published from its own network; it needs `ATRIUM_VIEWER` and `ATRIUM_MNEME_URL`, and `ATRIUM_ALLOWED_HOSTS` with the names it is reached by. `mise run image` builds it and checks it as it will run ([development.md](development.md)).

CI builds it on every push. Once the gate passes on a commit of `main` that is still main's head, it publishes it to `ghcr.io/notes-supply/atrium`, for x86_64, tagged with the commit, and signs it keyless as `.github/workflows/release.yml` on `main`.

## Releases

A tag `vX.Y.Z` on a commit of `main` runs `.github/workflows/binaries.yml`. It publishes a GitHub release with a static `atrium` for Linux on x86_64 and aarch64, each built in the image's own build stage on a runner of its architecture, `SHA256SUMS`, and a cosign bundle per binary, signed keyless as that workflow on the tag. Mneme's installer installs the latest release, and the README says how to [verify a download](../README.md#verify-a-download) by hand.

The same run tags the image CI published for the tag's commit as `X.Y.Z` and `latest`, which `examples/server` runs. Its signature is on its digest, which the new tags name too, so they need none of their own. When CI has published no image for the commit, because the tag came before its gate passed or a newer commit of `main` took its place, the run stops before it publishes anything and says so; rerun it once there is one, or tag a commit that has one.

## A cluster running Flux

A push to `main` publishes a release for a cluster once the gate passes on that commit. CI's release job pushes the image built by the `image` job to `ghcr.io/notes-supply/atrium`, tagged with the commit, and `tools/release.sh` pins that digest in `deploy/`, pushes `deploy/` as one OCI artifact to `ghcr.io/notes-supply/manifests/atrium`, signs the image and the artifact keyless as `.github/workflows/release.yml` on `main`, and points the artifact's `main` tag at it.

`deploy/` is a Kustomize base that holds only what a release may change: two pods spread over nodes, rolled one at a time, and their disruption budget. They run as UID 65532 on a read-only root, listen on port 8080, read Mneme at `http://mneme:8080` and serve the owner. A cluster running Flux can follow the artifact's `main` tag and apply only what cosign verifies, with the issuer `https://token.actions.githubusercontent.com` and this identity:

```
^https://github\.com/notes-supply/atrium/\.github/workflows/release\.yml@refs/heads/main$
```

Such a cluster adds its own specifics as an overlay: the host name in `ATRIUM_ALLOWED_HOSTS`, the time zone in `ATRIUM_TIME_ZONE`, the name of the server that holds the record in `ATRIUM_RECORD_NAME`, and the Services, Ingress, network policy and secrets around the pods. The project's own cluster does that in its own repository. It also checks every pod a release creates in namespace `atrium` and refuses it if it:

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
