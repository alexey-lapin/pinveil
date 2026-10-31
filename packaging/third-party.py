"""Writes THIRD-PARTY-NOTICES.txt for the release binaries of one target.

    python3 packaging/third-party.py <rust-target> <output>

Walks the locked dependency graph of `pinveil-server` and `pinveil` for the target (normal
dependencies only: what is linked into the shipped binaries) and copies every license,
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
ROOTS = ("pinveil-server", "pinveil-cli")

HEADER = """Third-party software in pinveil

pinveil itself is licensed under the Apache License 2.0 (see LICENSE and NOTICE). The
binaries also contain the Rust crates listed below, linked statically, including native
code some of them build and bundle:

- libsqlite3-sys bundles SQLite, which is in the public domain
  (https://www.sqlite.org/copyright.html).
- aws-lc-sys (in the CLI, through rustls) bundles AWS-LC; its license files below include
  the OpenSSL and BoringSSL notices it carries.

Each license text follows once, with the crates it covers.
"""


def shipped_packages(target):
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
    stack = [p["id"] for p in meta["packages"] if p["name"] in ROOTS]
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


def main(target, output):
    texts = {}  # digest -> (text, [crate labels])
    missing = []
    crates = shipped_packages(target)
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

    out = [HEADER, f"Crates ({len(crates)}):"]
    out += [f"  {p['name']} {p['version']}  {p.get('license') or ''}" for p in crates]
    for text, labels in sorted(texts.values(), key=lambda entry: entry[1][0]):
        out.append("\n" + "=" * 78)
        out += [f"Applies to {label}" for label in labels]
        out.append("-" * 78)
        out.append(text)
    pathlib.Path(output).write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
