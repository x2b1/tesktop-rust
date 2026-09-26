#!/usr/bin/env python3
"""Render the bundled ports' chat-bar button icons into the icon atlas.

The runtime draws every icon from one raster atlas, and tints it with the palette at paint
time. TestCord's chat-bar buttons are bespoke SVG paths, so they have to become atlas cells
before they can be drawn like anything else. This renders them once, into cells past the end
of the sheet, so no existing cell moves and no index changes meaning.

The glyphs are rendered white with an alpha channel, exactly like the rest of the sheet: the
colour is the app's to choose at paint time, which is what lets a toggle look the way
TestCord's does (its own colour off, the danger colour on) without baking a colour into an
asset.

    python3 tools/make-testcord-button-icons.py

Needs `rsvg-convert` on the path. The sources are quoted from the plugin they belong to.
"""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ATLAS = ROOT / "assets" / "icons" / "atlas.png"
INDEX = ROOT / "assets" / "icons" / "index.tsv"
TESTCORD = pathlib.Path("/home/x2b/Documents/DemonCum/TestCord-ref/src")

CELL = 64
COLUMNS = 8
# The sheet draws the glyph in 56 of every 64 pixels; match that so a plugin icon sits on
# the same optical size as the app's own.
GLYPH = 56
INSET = (CELL - GLYPH) // 2

# TestCord's chat-bar buttons, quoted from the plugin that owns each one. Only the ports that
# are bundled here are listed; a port with no button in TestCord gets no cell.
#
# `view_box` and `paths` are the plugin's own, and `tint` says how the app should colour it:
# `off` is the plain icon colour and `on` is what a switched-on button uses.
BUTTONS = [
    {
        "name": "ingtoninator",
        "plugin": "equicordplugins/ingtoninator",
        "view_box": "0 0 256 256",
        "transform": "translate(351 -153)",
        "paths": [
            'M-177.7,334.5c6.3-2.3,12.6-5.2,19.8-8.6c31.9-16.4,51.7-41.7,51.7-41.7s-32.5,0.6-64.4,17 c-4,1.7-7.5,4-10.9,5.7c5.7-7.5,12.1-16.4,18.7-25c25-37.1,31.3-77.3,31.3-77.3s-34.8,21-59.2,58.6c-5.2,7.5-9.8,14.9-13.8,22.7 c1.1-10.3,1.1-22.1,1.1-33.6c0-50-19.8-91.1-19.8-91.1s-19.8,40.5-19.8,91.1c0,12.1,0.6,23.3,1.1,33.6c-4-7.5-8.6-14.9-13.8-22.7 c-25-37.1-59.2-58.6-59.2-58.6s6.3,40,31.3,77.3c6.3,9.2,12.1,17.5,18.7,25c-3.4-2.3-7.5-4-10.9-5.7c-31.9-16.4-64.4-17-64.4-17 s19.8,25.6,51.7,41.7c6.9,3.4,13.2,6.3,19.8,8.6c-4,0.6-8,1.1-12.1,2.3c-30.5,6.4-53.2,23.9-53.2,23.9s27.3,7.5,58.6,1.1 c9.8-2.3,19.8-4.6,27.3-7.5c-1.1,1.1,15.8-8.6,21.6-14.4v60.4h8.6v-61.8c6.3,6.3,22.7,16.4,22.1,14.9c8,2.9,17.5,5.2,27.3,7.5 c30.8,6.3,58.6-1.1,58.6-1.1s-22.1-17.5-53.4-23.8C-169.6,335.7-173.7,335.1-177.7,334.5z',
        ],
    },
    {
        "name": "reverse-message",
        "plugin": "equicordplugins/talkInReverse",
        "view_box": "0 -960 960 960",
        "paths": [
            "M482-160q-134 0-228-93t-94-227v-7l-36 36q-11 11-28 11t-28-11q-11-11-11-28t11-28l104-104q12-12 28-12t28 12l104 104q11 11 11 28t-11 28q-11 11-28 11t-28-11l-36-36v7q0 100 70.5 170T482-240q16 0 31.5-2t30.5-7q17-5 32 1t23 21q8 16 1.5 31.5T577-175q-23 8-47 11.5t-48 3.5Zm-4-560q-16 0-31.5 2t-30.5 7q-17 5-32.5-1T360-733q-8-15-1.5-30.5T381-784q24-8 48-12t49-4q134 0 228 93t94 227v7l36-36q11-11 28-11t28 11q11 11 11 28t-11 28L788-349q-12 12-28 12t-28-12L628-453q-11-11-11-28t11-28q11-11 28-11t28 11l36 36v-7q0-100-70.5-170T478-720Z",
        ],
    },
    {
        "name": "signature",
        "plugin": "equicordplugins/signature",
        "view_box": "0 0 24 21.333",
        "paths": [
            "M2 4.621a.5.5 0 0 1 .854-.353l6.01 6.01c.126.126.17.31.15.487a2 2 0 1 0 1.751-1.751a.59.59 0 0 1-.487-.15l-6.01-6.01A.5.5 0 0 1 4.62 2H11a9 9 0 0 1 8.468 12.054l2.24 2.239a1 1 0 0 1 0 1.414l-4 4a1 1 0 0 1-1.415 0l-2.239-2.239A9 9 0 0 1 2 11z",
        ],
    },
]


def quoted_from_source(button: dict[str, object]) -> None:
    """Fail loudly if a glyph is no longer the one the plugin actually ships."""
    plugin = TESTCORD / str(button["plugin"])
    source = ""
    for name in ("index.ts", "index.tsx"):
        candidate = plugin / name
        if candidate.is_file():
            source = candidate.read_text(encoding="utf-8")
            break
    if not source:
        return
    for path in button["paths"]:  # type: ignore[union-attr]
        if str(path)[:40] not in source:
            raise SystemExit(
                f"{button['name']}: the path is no longer in {button['plugin']}. "
                "Re-quote it from the plugin rather than trusting this file."
            )


def svg_for(button: dict[str, object]) -> str:
    body = "\n".join(
        f'  <path fill="#ffffff" d="{path}"/>' for path in button["paths"]  # type: ignore[union-attr]
    )
    transform = button.get("transform")
    if transform:
        body = f'  <g transform="{transform}">\n{body}\n  </g>'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{GLYPH}" height="{GLYPH}" '
        f'viewBox="{button["view_box"]}">\n{body}\n</svg>\n'
    )


def main() -> int:
    for button in BUTTONS:
        quoted_from_source(button)
    try:
        from PIL import Image
    except ImportError:
        return "Pillow is needed to write the atlas"

    rows = [line.split("\t") for line in INDEX.read_text(encoding="utf-8").splitlines() if line]
    named = {name for name, _ in rows}
    taken = {int(cell) for _, cell in rows}
    sheet = Image.open(ATLAS).convert("RGBA")
    columns = sheet.width // CELL

    added: list[tuple[str, int]] = []
    with tempfile.TemporaryDirectory() as scratch:
        for button in BUTTONS:
            if str(button["name"]) in named:
                continue
            cell = next(index for index in range(sheet.width // CELL * (sheet.height // CELL)) if index not in taken)
            taken.add(cell)
            source = pathlib.Path(scratch) / f"{button['name']}.svg"
            source.write_text(svg_for(button), encoding="utf-8")
            rendered = pathlib.Path(scratch) / f"{button['name']}.png"
            subprocess.run(
                ["rsvg-convert", "-w", str(GLYPH), "-h", str(GLYPH), "-o", str(rendered), str(source)],
                check=True,
            )
            glyph = Image.open(rendered).convert("RGBA")
            x = (cell % columns) * CELL + INSET
            y = (cell // columns) * CELL + INSET
            sheet.alpha_composite(glyph, (x, y))
            added.append((str(button["name"]), cell))

    if not added:
        print("every plugin icon is already in the atlas")
        return 0
    for name, cell in added:
        rows.append([name, str(cell)])
    INDEX.write_text(
        "".join(f"{name}\t{cell}\n" for name, cell in rows),
        encoding="utf-8",
    )
    sheet.save(ATLAS, optimize=True)
    for name, cell in added:
        print(f"  {name} -> cell {cell} (row {cell // columns}, column {cell % columns})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
