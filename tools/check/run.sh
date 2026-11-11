#!/bin/sh
# Runs a check against `atrium --demo` in Playwright's 1.64.1 image: parity.mjs or axe.mjs.
# Starts the demo on PORT and the away demo on PORT+1, both of which must be free, and stops
# them after. PROTOTYPE is the design's prototype folder; NODE_MODULES holds Playwright 1.64.1
# and @axe-core/playwright, mounted read-only. Results land in target/check/<check>.
#
#   tools/check/run.sh parity [route ...]
#   tools/check/run.sh axe
set -eu
check="$1"
shift
here=$(cd "$(dirname "$0")/../.." && pwd)
port="${PORT:-18740}"
away=$((port + 1))
prototype="${PROTOTYPE:-/home/blackbox/work/atrium-concepts/atrium}"
modules="${NODE_MODULES:-/home/blackbox/work/shebang-web/apps/site/checks/node_modules}"
out="$here/target/check/$check"
for p in "$port" "$away"; do
  if ss -ltn | awk '{print $4}' | grep -q ":$p\$"; then
    echo "port $p is in use; set PORT to a free one" >&2
    exit 1
  fi
done
cargo build --quiet --locked --manifest-path "$here/Cargo.toml" -p atrium
bin="$here/target/debug/atrium"
"$bin" --demo --viewer tailnet --addr "127.0.0.1:$port" --allowed-hosts 127.0.0.1 2>/dev/null &
wiki=$!
"$bin" --demo away --viewer tailnet --addr "127.0.0.1:$away" --allowed-hosts 127.0.0.1 2>/dev/null &
gone=$!
trap 'kill "$wiki" "$gone" 2>/dev/null || true' EXIT
for _ in $(seq 50); do curl -fsS "http://127.0.0.1:$away/healthz" >/dev/null 2>&1 && break; sleep 0.1; done
rm -rf "$out"
mkdir -p "$out"
docker run --rm --cpus=2 --memory=4g --init --ipc=host --network host \
  --user "$(id -u):$(id -g)" --env HOME=/tmp \
  --volume "$here/tools/check:/check:ro" --volume "$modules:/node_modules:ro" \
  --volume "$prototype:/prototype:ro" --volume "$out:/out" \
  --workdir /check mcr.microsoft.com/playwright:v1.64.1-noble \
  node "$check.mjs" /prototype "http://127.0.0.1:$port" /out "$@"
