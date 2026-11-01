"""Writes THIRD-PARTY-NOTICES.txt for one release binary of one target.

    python3 packaging/third-party.py <pinveil-server|pinveil-cli> <rust-target> <output>

Walks the locked dependency graph of that crate for the target (normal dependencies only:
what is linked into the shipped binary) and copies every license,
notice, and copyright file each crate ships, from the sources Cargo downloaded. Identical
texts are printed once with the crates they cover. Fails if a crate has no such file, so
nothing is shipped without its notice.
"""

import hashlib
import json
import os
import pathlib
import subprocess
import sys

NOTICE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE")
BINARIES = {"pinveil-server": "pinveil-server", "pinveil-cli": "pinveil"}

# Native code a crate builds and bundles, noted in the header when the crate is linked.
BUNDLED = {
    "libsqlite3-sys": "libsqlite3-sys bundles SQLite, which is in the public domain\n"
    "  (https://www.sqlite.org/copyright.html).",
    "aws-lc-sys": "aws-lc-sys (through rustls) bundles AWS-LC; its license files below\n"
    "  include the OpenSSL and BoringSSL notices it carries.",
}


def shipped_packages(root, target):
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    stack = [p["id"] for p in meta["packages"] if p["name"] == root]
    seen = set()
    while stack:
        node = stack.pop()
        if node in seen:
            continue
        seen.add(node)
        for dep in nodes[node]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    # Workspace crates (no source) are pinveil itself.
    return sorted(
        (packages[i] for i in seen if packages[i]["source"] is not None),
        key=lambda p: (p["name"], p["version"]),
    )


def header(binary, crates):
    names = {p["name"] for p in crates}
    bundled = [f"- {note}" for crate, note in BUNDLED.items() if crate in names]
    lines = [
        f"Third-party software in {binary}",
        "",
        "pinveil is licensed under the MIT License (see LICENSE and NOTICE). This",
        "binary also contains the Rust crates listed below, linked statically"
        + (", including native\ncode some of them build and bundle:" if bundled else "."),
    ]
    if bundled:
        lines += [""] + bundled
    return "\n".join(lines + ["", "Each license text follows once, with the crates it covers.", ""])


def main(root, target, output):
    if root not in BINARIES:
        raise SystemExit(f"unknown crate {root}; expected one of {', '.join(BINARIES)}")
    texts = {}  # digest -> (text, [crate labels])
    missing = []
    crates = shipped_packages(root, target)
    for package in crates:
        label = f"{package['name']} {package['version']} ({package.get('license') or 'see text'})"
        directory = pathlib.Path(package["manifest_path"]).parent
        files = sorted(
            f for f in os.listdir(directory) if f.upper().startswith(NOTICE_PREFIXES)
        )
        if not files:
            missing.append(label)
        for name in files:
            path = directory / name
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8", errors="replace").strip()
            digest = hashlib.sha256(text.encode()).hexdigest()
            texts.setdefault(digest, (text, []))[1].append(f"{label}: {name}")
    if missing:
        raise SystemExit("no license or notice file in: " + ", ".join(missing))

    out = [header(BINARIES[root], crates), f"Crates ({len(crates)}):"]
    out += [f"  {p['name']} {p['version']}  {p.get('license') or ''}" for p in crates]
    for text, labels in sorted(texts.values(), key=lambda entry: entry[1][0]):
        out.append("\n" + "=" * 78)
        out += [f"Applies to {label}" for label in labels]
        out.append("-" * 78)
        out.append(text)
    pathlib.Path(output).write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
