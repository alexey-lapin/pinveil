/**
 * The export-name policy: turns names from a manifest into names that are safe to save on any
 * supported platform. Mirrors `pinveil-core/src/export_names.rs`; both are pinned by
 * `tests/vectors/export-names.json`.
 */

import { MAX_NAME_BYTES } from "./protocol.js";

// Windows-invalid characters plus C0 and C1 control characters (Rust's `char::is_control`).
const INVALID = /[/\\:*?"<>|\u0000-\u001f\u007f-\u009f]/gu;
// Reserved on Windows with or without an extension, in any case. Microsoft's naming rules also
// reserve COM and LPT followed by superscript one, two, or three.
const WINDOWS_DEVICES = new Set([
    "con", "prn", "aux", "nul",
    ...["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].flatMap((n) => [`com${n}`, `lpt${n}`]),
]);
const encoder = new TextEncoder();

/**
 * Maps each name to a safe, unique export name, in order. Names that differ only by case count
 * as duplicates, because Windows and macOS filesystems usually ignore case.
 */
export function exportNames(names) {
    const used = new Set();
    return names.map((name, index) => {
        const unique = disambiguate(sanitize(name, index), used);
        used.add(unique.toLowerCase());
        return unique;
    });
}

function sanitize(name, index) {
    let safe = trimEnd(name.replace(INVALID, "_"));
    if (safe === "") {
        return `file-${index + 1}`;
    }
    const stem = safe.split(".")[0].replace(/ +$/u, "");
    if (WINDOWS_DEVICES.has(stem.toLowerCase())) {
        safe = `_${safe}`;
    }
    return trimEnd(truncate(safe, MAX_NAME_BYTES));
}

function disambiguate(name, used) {
    if (!used.has(name.toLowerCase())) {
        return name;
    }
    const dot = name.lastIndexOf(".");
    const [stem, extension] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
    for (let n = 2; ; n += 1) {
        const suffix = ` (${n})`;
        // Keep the extension only if at least one byte of the stem still fits beside it;
        // otherwise number the whole name, cut to size.
        const keep = utf8Length(suffix) + utf8Length(extension) < MAX_NAME_BYTES;
        const [base, ext] = keep ? [stem, extension] : [name, ""];
        const room = MAX_NAME_BYTES - utf8Length(suffix) - utf8Length(ext);
        const candidate = `${truncate(base, room)}${suffix}${ext}`;
        if (!used.has(candidate.toLowerCase())) {
            return candidate;
        }
    }
}

/** Windows drops trailing dots and spaces, so names must not end with them. */
function trimEnd(name) {
    return name.replace(/[. ]+$/u, "");
}

function utf8Length(text) {
    return encoder.encode(text).length;
}

/** The longest prefix of `text` that fits in `maxBytes` of UTF-8, cut at a character boundary. */
function truncate(text, maxBytes) {
    if (utf8Length(text) <= maxBytes) {
        return text;
    }
    let out = "";
    let bytes = 0;
    for (const char of text) {
        const size = utf8Length(char);
        if (bytes + size > maxBytes) {
            break;
        }
        out += char;
        bytes += size;
    }
    return out;
}
