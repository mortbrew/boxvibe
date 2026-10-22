#!/bin/sh
# Renders demo-mode screenshots into screenshots/: boxvibe draws one frame
# with ratatui's TestBackend as HTML (--snapshot), headless Chrome makes it a PNG.
#   tools/screenshots.sh   (needs google-chrome or chromium, DejaVu Sans Mono, Noto Color Emoji)
set -eu
cd "$(dirname "$0")/.."
cargo build --release --quiet
BIN=target/release/boxvibe
CHROME=$(command -v google-chrome || command -v chromium || command -v chromium-browser)
COLS=120 ROWS=36
# DejaVu Sans Mono at 16px: 9.63px per column, rows 1.17em; 10px padding around.
W=$(( COLS * 963 / 100 + 22 )) H=$(( ROWS * 1872 / 100 + 22 ))
TMP=$(mktemp -d)
shot() { # name theme icons screen [config file]
    "$BIN" --config "${5:-$TMP/none.toml}" --snapshot "$TMP/$1.html" --theme "$2" --icons "$3" --screen "$4" --cols $COLS --rows $ROWS
    "$CHROME" --headless=new --no-sandbox --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
        --window-size=$W,$H --screenshot="screenshots/$1.png" "file://$TMP/$1.html" 2>/dev/null
    echo "screenshots/$1.png"
}
mkdir -p screenshots
shot gruvbox-main gruvbox emoji main
shot gruvbox-settings gruvbox emoji settings
shot gruvbox-about gruvbox emoji about
shot gruvbox-about-restart gruvbox emoji about-restart
printf '[ui]\nborder_style = "rounded"\n' > "$TMP/rounded.toml"
shot gruvbox-rounded gruvbox emoji main "$TMP/rounded.toml"
rm -rf "$TMP"
