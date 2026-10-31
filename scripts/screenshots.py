#!/usr/bin/env python3
"""Draw target/screenshots/*.json (from `cargo test --release screenshots --
--ignored`) to docs/screenshots/*.png. Needs Pillow, fontTools, DejaVu Sans Mono and
the Nerd Font symbols (Symbols Nerd Font Mono)."""
import json, pathlib, sys
from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
SIZE, CW, CH, PAD = 16, 10, 20, 16
# Icons DejaVu lacks come from the Nerd Font symbols (boxvibe's default icons).
NERD = [pathlib.Path.home() / ".local/share/fonts/nerd/SymbolsNerdFontMono-Regular.ttf",
        pathlib.Path("/usr/share/fonts/TTF/SymbolsNerdFontMono-Regular.ttf")]
font, bold = ImageFont.truetype(FONT, SIZE), ImageFont.truetype(BOLD, SIZE)
nerd = next((ImageFont.truetype(str(p), SIZE) for p in NERD if p.exists()), None)
from fontTools.ttLib import TTFont
HAVE = set(TTFont(FONT).getBestCmap())


def color(c, default):
    return c if c.startswith("#") else default


def draw(doc):
    cols, rows, cells = doc["cols"], doc["rows"], doc["cells"]
    base = color(cells[0][2], "#1a1b26")
    img = Image.new("RGB", (cols * CW + 2 * PAD, rows * CH + 2 * PAD), base)
    d = ImageDraw.Draw(img)
    for i, (sym, fg, bg, b) in enumerate(cells):
        x, y = PAD + (i % cols) * CW, PAD + (i // cols) * CH
        fg, bg = color(fg, "#c0caf5"), color(bg, base)
        d.rectangle([x, y, x + CW - 1, y + CH - 1], fill=bg)
        if sym in ("", " "):
            continue
        # Blocks and box lines are drawn as shapes so they meet with no gaps.
        if sym == "▀":
            d.rectangle([x, y, x + CW - 1, y + CH // 2 - 1], fill=fg)
        elif sym == "▄":
            d.rectangle([x, y + CH // 2, x + CW - 1, y + CH - 1], fill=fg)
        elif sym == "█":
            d.rectangle([x, y, x + CW - 1, y + CH - 1], fill=fg)
        elif sym in "─━":
            d.line([x, y + CH // 2, x + CW, y + CH // 2], fill=fg, width=1 if sym == "─" else 3)
        elif sym in "│┃":
            d.line([x + CW // 2, y, x + CW // 2, y + CH], fill=fg, width=1 if sym == "│" else 3)
        elif sym in "╭╮╰╯┌┐└┘":
            cx, cy = x + CW // 2, y + CH // 2
            d.line([cx, cy, x + CW if sym in "╭┌╰└" else x, cy], fill=fg)
            d.line([cx, cy, cx, y + CH if sym in "╭┌╮┐" else y], fill=fg)
        else:
            f = bold if b else font
            if nerd and any(ord(ch) not in HAVE for ch in sym):
                f = nerd
            d.text((x, y + 1), sym, font=f, fill=fg)
    return img


out = ROOT / "docs/screenshots"
out.mkdir(parents=True, exist_ok=True)
for f in sorted((ROOT / "target/screenshots").glob("*.json")):
    draw(json.loads(f.read_text())).save(out / (f.stem + ".png"), optimize=True)
    print(out / (f.stem + ".png"))
