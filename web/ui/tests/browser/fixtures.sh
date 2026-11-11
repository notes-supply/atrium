#!/bin/sh
# Copies the prototype's pages into fixtures/ as the server will write them: no boot script,
# the one loader with the bundles' URLs on its element, the palette's page list named in
# data-pages, and no since module (the server keeps the last visit). Rerun it when the
# prototype changes: web/ui/tests/browser/fixtures.sh /path/to/atrium-concepts/atrium
set -eu
from="${1:?the prototype folder}"
here="$(cd "$(dirname "$0")" && pwd)"
out="$here/fixtures"
rm -rf "$out"
mkdir -p "$out/assets"
cp -R "$from/fonts" "$out/fonts"
cp -R "$from/assets/dither" "$out/assets/dither"
cp "$from"/assets/*.css "$from/assets/pages.json" "$out/assets/"
loader='<script type="module" src="/web/loader.js" data-ui="/web/ui.js" data-ui-wasm="/web/ui_bg.wasm" data-field="/web/field.js" data-field-wasm="/web/field_bg.wasm"></script>'
for page in "$from"/*.html; do
  sed -e '/<script src="assets\/boot.js"><\/script>/d' \
    -e "s#<script type=\"module\" src=\"assets/load.js\"></script>#$loader#" \
    -e 's#data-module="palette"#data-module="palette" data-pages="assets/pages.json"#g' \
    -e 's#data-module="keys light carry since"#data-module="keys light carry"#' \
    "$page" >"$out/$(basename "$page")"
done
# Continue reading lives in the browser: the server writes only the empty, hidden slots.
perl -0pi -e 's#<article class="tile resume surface" data-surface aria-labelledby="resume-h">\s*(<header.*?</header>).*?</article>#<article class="tile resume surface" data-surface aria-labelledby="resume-h" data-module="continue" hidden>\n      $1\n    </article>#s' "$out/index.html"
perl -0pi -e 's#<p class="resume-note">(<svg.*?</svg>)<span>.*?</span></p>#<p class="resume-note" hidden>$1<span></span></p>#s' "$out/page.html"
# The progress bar's steps, and hidden slots that stay hidden, which the server's
# stylesheet will carry.
{
  for p in $(seq 0 5 100); do
    echo ".pb-fill.pct-$p { width: $p%; } .pb-n.pct-$p { left: $p%; }"
  done
  echo ".tile[hidden], .resume-note[hidden] { display: none; }"
} >>"$out/assets/drawn.css"
grep -L 'data-ui=' "$out"/*.html | sed 's/^/no loader: /' >&2
