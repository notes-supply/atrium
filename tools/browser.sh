#!/bin/sh
# Runs a command in Playwright's 1.64.1 image against the real server: `atrium --demo` on
# PORT and `atrium --demo away` on PORT+1, both of which, with PORT+2, must be free. The
# repository is mounted at its own path, with the npm packages package-lock.json pins,
# installed by `npm ci` in the image whenever the lock changes. The command sees
# ATRIUM_URL, ATRIUM_AWAY_URL and, when PROTOTYPE names the design's prototype folder,
# PROTOTYPE and PROTOTYPE_PORT. Two CPUs and 4 GB, or the cores CPUSET names, which the
# servers share. One core is about as slow as GitHub's runner.
#
#   tools/browser.sh npx playwright test -c web/ui/tests/browser/playwright.config.mjs
#   tools/browser.sh node tools/check/parity.mjs
#   CPUSET=7 mise run test:browser
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
image=mcr.microsoft.com/playwright:v1.64.1-noble
port="${PORT:-18760}"
away=$((port + 1))
for p in "$port" "$away" $((port + 2)); do
  if ss -ltn | awk '{print $4}' | grep -q ":$p\$"; then
    echo "port $p is in use; set PORT to a free one" >&2
    exit 1
  fi
done
cargo build --quiet --locked -p atrium
mkdir -p target/browser
pin=""
[ -n "${CPUSET:-}" ] && pin="taskset -c $CPUSET"
$pin target/debug/atrium --demo --viewer tailnet --addr "127.0.0.1:$port" --allowed-hosts 127.0.0.1 \
  2>target/browser/demo.log &
demo=$!
$pin target/debug/atrium --demo away --viewer tailnet --addr "127.0.0.1:$away" --allowed-hosts 127.0.0.1 \
  2>target/browser/away.log &
gone=$!
trap 'kill "$demo" "$gone" 2>/dev/null || true' EXIT
for p in "$port" "$away"; do
  for _ in $(seq 100); do curl -fsS "http://127.0.0.1:$p/healthz" >/dev/null 2>&1 && break; sleep 0.1; done
done

# A CPU quota throttles the software renderer into slow frames that are not real, so a
# benchmark pins cores instead.
limit="--cpus=2"
[ -n "${CPUSET:-}" ] && limit="--cpuset-cpus=$CPUSET"
run() {
  docker run --rm $limit --memory=4g --init --ipc=host --network host \
    --user "$(id -u):$(id -g)" --env HOME=/tmp --env CI="${CI:-}" \
    --volume "$here:$here" --workdir "$here" "$@"
}
if ! cmp -s package-lock.json node_modules/.atrium-lock; then
  run "$image" sh -c 'npm ci --ignore-scripts --no-audit --no-fund && cp package-lock.json node_modules/.atrium-lock'
fi
prototype=""
if [ -n "${PROTOTYPE:-}" ]; then
  prototype="--volume $PROTOTYPE:$PROTOTYPE:ro --env PROTOTYPE=$PROTOTYPE --env PROTOTYPE_PORT=$((port + 2))"
fi
run --env "ATRIUM_URL=http://127.0.0.1:$port" --env "ATRIUM_AWAY_URL=http://127.0.0.1:$away" \
  $prototype "$image" "$@"
