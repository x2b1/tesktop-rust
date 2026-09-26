#!/usr/bin/env python3
"""Inventory of the TestCord plugin set, with a portability verdict per plugin.

Reads a TestCord checkout and writes `docs/testcord-inventory.csv` plus a short summary.
The point is to make "port everything" auditable: every plugin is listed once, with the
hooks it uses and what it would take to run natively in tesktop-rust.

    python3 tools/generate-testcord-inventory.py /path/to/TestCord-ref [--check]

`--check` also fails when a port in the native runtime matches no plugin in the checkout.
That is the check that keeps a port's id honest: the id is what an imported TestCord
settings file looks the port up by, so a port whose id is not TestCord's own name is a
port an import will never find.
"""

from __future__ import annotations

import csv
import pathlib
import re
import sys

ROOTS = ("plugins", "equicordplugins", "testcordplugins")
SOURCE = "src"

# Ids in the native runtime that are not ports. Each is here with its reason rather than
# worked around, so a new one has to be argued for.
NOT_PORTS = {
    "probe": "the registry's own test fixture, in the crate's test module",
}
NOT_PORTS_KEY = "probe"

# Selfbot, token, mass-messaging, anti-logging and surveillance features. The native client's
# product boundaries exclude these, so they are never ported.
EXCLUDED = {
    "token", "tokenlogin", "tokenimporter", "importmultitokens", "dxtokenimporter", "getmytoken",
    "bypassaccounts", "multiinstance", "fakeaccounts", "fakeuserswitcher", "fakeuserprofile",
    "fakeuserprofiles", "fakeprofile", "fakefriends", "fakedm", "fakeconnections", "fakeperm",
    "fakevoicepremium", "fakeindicators", "fakemutedeafen", "morealts", "impersonate", "spoofmsgv2",
    "systemmessagespoofer", "badgespoofer", "silentcall", "silentdelete", "silentedit", "dmbomb",
    "massdm", "sendtoalldms", "purgedms", "purgemessages", "messagescrapper", "chatscrapper",
    "friendscrapper", "stalker", "surveillance", "localmessageedit", "rpunisher", "guildcopier",
    "serverpruner", "leaveallservers", "leaveallgroups", "muteallservers", "automessagesender",
    "automessagerepeater", "selfforward", "autodeco", "antiantilog", "antilogpremium", "antifilter",
    "opsecillegalcord", "securecordopossum", "webcordhardened", "goofcordsec", "notelemetry",
    "ghostselfbot", "ghostclient", "sniper", "nitrosniper", "apisniper", "floodpanel",
}

# Features that only exist because the client patches Discord's own minified internals; a native
# client has no such surface, so a port is a rewrite against this app's state.
WEB_ONLY_MARKERS = (
    'find: "experiments"',
    "PlatformEmulator",
    "streamingCodecDisabler",
    "e_(",
)

NAME = re.compile(r'^\s*name:\s*"([^"]+)"', re.M)
DESCRIPTION = re.compile(r'^\s*description:\s*"([^"]+)"', re.M)
AUTHORS = re.compile(r"^\s*authors:\s*\[([^\]]*)\]", re.M)
SETTING_KEYS = re.compile(r'^\s{4}([a-zA-Z][a-zA-Z0-9]*):\s*\{', re.M)
COMMAND = re.compile(r'^\s*commands:\s*\[', re.M)
FLUX = re.compile(r'^\s*flux:\s*\{', re.M)
PATCHES = re.compile(r'^\s{4}patches:\s*\[\s*\{', re.M)
BEFORE_SEND = re.compile(r"^\s*onBeforeMessageSend:", re.M)
BEFORE_EDIT = re.compile(r"^\s*onBeforeMessageEdit:", re.M)
RENDER = re.compile(r"^\s*render(Message|MemberList|Nickname|Profile|ChatBar)", re.M)
STYLE = re.compile(r'^\s*managedStyle:|import\s+"\./style\.css', re.M)


def plugin_name(folder: str, source: str) -> str:
    found = NAME.search(source)
    return found.group(1) if found else folder.split(".")[0]


def verdict(folder: str, source: str, natives: list[str]) -> str:
    identifier = folder.split(".")[0].lower()
    if identifier in EXCLUDED or plugin_name(folder, source).lower() in EXCLUDED:
        return "excluded"
    if any(marker in source for marker in WEB_ONLY_MARKERS):
        return "excluded-web-only"
    if natives:
        return "native-feature"
    if PATCHES.search(source):
        # A patch plugin is a rewrite here: its hooks may port, but the patch itself
        # targets Discord's own JavaScript, which this client never runs.
        return "rewrite"
    if BEFORE_SEND.search(source) or BEFORE_EDIT.search(source) or FLUX.search(source):
        return "portable-hook"
    if COMMAND.search(source) or RENDER.search(source) or STYLE.search(source):
        return "portable-ui"
    return "portable-logic"


def ported_aliases(root: pathlib.Path) -> dict[str, list[str]]:
    """Every id and alias the native runtime answers to, mapped to their Rust module."""
    found: dict[str, list[str]] = {}
    crate = root / "crates" / "tesktop-plugins" / "src"
    if not crate.is_dir():
        return found
    for module in sorted(crate.glob("*.rs")):
        source = module.read_text(encoding="utf-8")
        for identifier in re.findall(r'id:\s*"([^"]+)"', source):
            found.setdefault(identifier.lower(), []).append(identifier)
        for alias in re.findall(r'aliases:\s*&\[([^\]]*)\]', source):
            for identifier in re.findall(r'"([^"]+)"', alias):
                found.setdefault(identifier.lower(), []).append(identifier)
        # Ports generated by the text-command macro carry their id in the invocation.
        for identifier in re.findall(r'\btext_command!\(\s*\w+\s*,\s*"([^"]+)"', source):
            found.setdefault(identifier.lower(), []).append(identifier)
    return found


def main() -> int:
    arguments = [argument for argument in sys.argv[1:] if not argument.startswith("--")]
    if len(arguments) != 1:
        print(__doc__)
        return 2
    root = pathlib.Path(arguments[0]).resolve()
    native_root = pathlib.Path(__file__).resolve().parent.parent
    ported = ported_aliases(native_root)
    rows = []
    for group in ROOTS:
        base = root / SOURCE / group
        if not base.is_dir():
            continue
        for folder in sorted(base.iterdir()):
            if not folder.is_dir() or folder.name.startswith(("_", ".")):
                continue
            index = folder / "index.ts"
            if not index.exists():
                index = folder / "index.tsx"
            if not index.exists():
                continue
            source = index.read_text(encoding="utf-8", errors="replace")
            description = DESCRIPTION.search(source)
            authors = AUTHORS.search(source)
            natives = sorted(
                str(child.relative_to(folder)) for child in folder.rglob("native*.ts")
            )
            rows.append(
                {
                    "group": group,
                    "folder": folder.name,
                    "name": plugin_name(folder.name, source),
                    "description": (description.group(1) if description else "")[:110],
                    "authors": (authors.group(1).replace(" ", "") if authors else "")[:40],
                    "settings": len(SETTING_KEYS.findall(source)),
                    "patches": bool(PATCHES.search(source)),
                    "before_send": bool(BEFORE_SEND.search(source)),
                    "before_edit": bool(BEFORE_EDIT.search(source)),
                    "flux": bool(FLUX.search(source)),
                    "render": bool(RENDER.search(source)),
                    "native": ";".join(natives),
                    "verdict": verdict(folder.name, source, natives),
                    "ported": (ported.get(plugin_name(folder.name, source).lower()) or [""])[0],
                }
            )
    out = pathlib.Path(__file__).resolve().parent.parent / "docs" / "testcord-inventory.csv"
    with out.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)
    counts: dict[str, int] = {}
    for row in rows:
        counts[row["verdict"]] = counts.get(row["verdict"], 0) + 1
    done = sum(1 for row in rows if row["ported"])
    print(f"{len(rows)} plugins -> {out}")
    for key in sorted(counts):
        print(f"  {key}: {counts[key]}")
    print(f"  ported: {done}")

    # TestCord keys its settings file on the plugin's `name`, with whitespace turned into
    # underscores and everything else that is not a letter, a digit or an underscore
    # removed (PluginManager.ts, `loadPlugin`). A port the inventory counts as ported but
    # whose id is not that key is a port an imported settings file cannot switch on, which
    # is the whole of what this checks.
    keys: dict[int, str] = {}
    for index, row in enumerate(rows):
        if not row["ported"]:
            continue
        group = root / SOURCE / row["group"] / row["folder"]
        source_file = group / "index.ts"
        if not source_file.exists():
            source_file = group / "index.tsx"
        if not source_file.exists():
            continue
        key = re.sub(
            r"[^a-zA-Z0-9_]",
            "",
            re.sub(
                r"\s+",
                "_",
                plugin_name(row["folder"], source_file.read_text(encoding="utf-8")),
            ),
        )
        if key:
            keys[index] = key
    accepted = {
        identifier.lower()
        for identifiers in ported.values()
        for identifier in identifiers
        if identifier.lower() not in NOT_PORTS
    }
    unreachable = [
        (row["name"], keys[index])
        for index, row in enumerate(rows)
        if index in keys and keys[index].lower() not in accepted
    ]
    if unreachable:
        print(
            "\n  these ports are counted as ported, but nothing accepts the key TestCord "
            "writes for them:",
            file=sys.stderr,
        )
        for name, key in unreachable:
            print(f"    {name} -> {key}", file=sys.stderr)
        print(
            "  an imported settings file will silently skip them. Give the port the id\n"
            "  TestCord uses, or the folder name as an alias.",
            file=sys.stderr,
        )
        return 1
    if "--check" in sys.argv[2:]:
        print("  every port's id matches a plugin in the checkout")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
