#!/usr/bin/env python3
"""How close each bundled port is to the plugin it was ported from.

"Port everything" is only meaningful if what arrives is the thing that was there. This
compares, for every ported plugin, the settings it declares against the settings the original
declares, and reports the ones that are missing. A missing setting is not automatically a
missing feature — a port may have folded two settings into one field, the way one multiline
box can stand in for a repeating editor — so the report is a worklist, not a verdict.

    python3 tools/audit-port-fidelity.py /path/to/TestCord-ref

Add `--strict` to fail when any ported plugin is missing a setting, which is what a build
would use to stop the gap growing.
"""

from __future__ import annotations

import csv
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
INVENTORY = ROOT / "docs" / "testcord-inventory.csv"
NATIVE = ROOT / "crates" / "tesktop-plugins" / "src"

# Settings the original declares that no port can honestly carry, with the reason. Keeping
# these here means the list is argued for rather than quietly skipped.
CARRIED_ELSEWHERE = {
    "showIcon": "the chat-bar button this port draws is gated by the same switch",
    "keybind": "the app's own keybind settings, not one of the port's",
    "location": "this client has one place for a button, not a choice of places",
    "debug": "a diagnostics switch, not behaviour",
    "debugMode": "a diagnostics switch, not behaviour",
}


def original_settings(source: str) -> list[str]:
    """Every setting key the original declares, in order.

    Only the keys inside the settings declaration count. A plugin's source is full of other
    `name: {` object literals — types, rule shapes, saved records — and counting those reports
    gaps that do not exist, which is worse than reporting none.
    """
    # The call, not the import: the import is followed by the plugin's types, and those are
    # `name: {` literals that are not settings.
    start = source.find("definePluginSettings(")
    if start < 0:
        return []
    body = source[start:]
    keys: list[str] = []
    for match in re.finditer(r"(\w+):\s*\{", body):
        key = match.group(1)
        # Only count it if the block that follows declares a setting type.
        tail = body[match.end() : match.end() + 400]
        if "OptionType." not in tail:
            continue
        if key not in keys:
            keys.append(key)
    return keys


def port_settings(module: pathlib.Path) -> set[str]:
    """Every setting key a port declares, across the whole crate."""
    keys: set[str] = set()
    for path in NATIVE.glob("*.rs"):
        source = path.read_text(encoding="utf-8")
        for match in re.finditer(r'key:\s*"([^"]+)"', source):
            keys.add(match.group(1))
    return keys


def port_ids() -> dict[str, str]:
    """The id of every port, mapped to the file that declares it."""
    found: dict[str, str] = {}
    for path in sorted(NATIVE.glob("*.rs")):
        source = path.read_text(encoding="utf-8")
        for match in re.finditer(r'fn meta\(&self\) -> Meta \{\s*Meta \{\s*id:\s*"([^"]+)"', source):
            found[match.group(1)] = path.name
    return found


def main() -> int:
    arguments = [argument for argument in sys.argv[1:] if not argument.startswith("--")]
    strict = "--strict" in sys.argv
    if len(arguments) != 1:
        print(__doc__)
        return 2
    root = pathlib.Path(arguments[0]).resolve()

    rows = list(csv.DictReader(INVENTORY.open(encoding="utf-8")))
    declared = port_settings(NATIVE)
    modules = port_ids()
    missing_total = 0
    audited = 0

    for row in rows:
        identifier = row["ported"]
        if not identifier or identifier not in modules:
            continue
        folder = root / "src" / row["group"] / row["folder"]
        source = ""
        for name in ("index.ts", "index.tsx"):
            candidate = folder / name
            if candidate.is_file():
                source = candidate.read_text(encoding="utf-8", errors="ignore")
        if not source:
            continue
        wanted = original_settings(source)
        if not wanted:
            continue
        audited += 1
        gaps = [key for key in wanted if key not in declared]
        if not gaps:
            continue
        missing_total += len(gaps)
        unexplained = [key for key in gaps if key not in CARRIED_ELSEWHERE]
        marker = " " if not unexplained else "*"
        print(f"{marker} {identifier:26s} {row['group']}/{row['folder']}")
        for key in gaps:
            note = CARRIED_ELSEWHERE.get(key, "")
            print(f"      {key:24s} {note}")

    print()
    print(f"{audited} ported plugins checked, {missing_total} settings not carried over")
    print("  * marks a gap with no stated reason")
    if strict and missing_total:
        print("  --strict: failing because gaps remain", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
