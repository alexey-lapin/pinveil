/**
 * A store-only zip writer for "Download all" (APP.md, "Web UI"). It follows PKWARE APPNOTE:
 * method 0 (stored), CRC-32, and general-purpose bit 11 so names are read as UTF-8. It writes
 * ZIP32 only, with no Zip64 records, so it refuses archives past the ZIP32 bounds instead of
 * writing a broken one. The server refuses to start with limits that could reach them.
 *
 * The archive is returned as a list of parts for `new Blob(parts)`. File data are passed
 * through as the same views, so the whole message is never copied into one buffer.
 */

const VERSION = 20; // 2.0: the version that introduced the stored method and UTF-8 names.
const UTF8_NAMES = 0x0800;
const U32_MAX = 0xffff_ffff;
const U16_MAX = 0xffff;

const encoder = new TextEncoder();

const CRC_TABLE = (() => {
    const table = new Uint32Array(256);
    for (let n = 0; n < 256; n += 1) {
        let c = n;
        for (let k = 0; k < 8; k += 1) {
            c = c & 1 ? 0xedb8_8320 ^ (c >>> 1) : c >>> 1;
        }
        table[n] = c >>> 0;
    }
    return table;
})();

/** CRC-32 (ISO 3309, the one zip uses) of `bytes`. */
export function crc32(bytes) {
    let crc = U32_MAX;
    for (let i = 0; i < bytes.length; i += 1) {
        crc = CRC_TABLE[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8);
    }
    return (crc ^ U32_MAX) >>> 0;
}

/** MS-DOS time and date, in local time as zip readers expect. Clamped to 1980..2107. */
function dosDateTime(date) {
    const year = Math.min(Math.max(date.getFullYear(), 1980), 2107);
    const time = (date.getHours() << 11) | (date.getMinutes() << 5) | Math.floor(date.getSeconds() / 2);
    const day = ((year - 1980) << 9) | ((date.getMonth() + 1) << 5) | date.getDate();
    return { time, day };
}

/**
 * Builds a zip of `entries` (`[{name, data: Uint8Array}]`, names already passed through the
 * export-name policy) and returns its parts. Throws a `RangeError` past the ZIP32 bounds.
 */
export function zipParts(entries, modified = new Date()) {
    if (entries.length > U16_MAX) {
        throw new RangeError("Too many files for a zip archive.");
    }
    // Check every bound before any CRC work, so an oversized set fails at once.
    const names = entries.map(({ name }) => encoder.encode(name));
    let total = 0;
    for (const [i, { data }] of entries.entries()) {
        if (names[i].length > U16_MAX || data.length > U32_MAX) {
            throw new RangeError("A file is too large for a zip archive.");
        }
        total += 30 + names[i].length + data.length + 46 + names[i].length;
    }
    if (total > U32_MAX) {
        throw new RangeError("The files are too large for a zip archive.");
    }

    const { time, day } = dosDateTime(modified);
    const parts = [];
    const central = [];
    let offset = 0;

    for (const [i, { data }] of entries.entries()) {
        const nameBytes = names[i];
        const crc = crc32(data);
        const local = new Uint8Array(30 + nameBytes.length);
        const l = new DataView(local.buffer);
        l.setUint32(0, 0x0403_4b50, true);
        l.setUint16(4, VERSION, true);
        l.setUint16(6, UTF8_NAMES, true);
        l.setUint16(8, 0, true); // stored
        l.setUint16(10, time, true);
        l.setUint16(12, day, true);
        l.setUint32(14, crc, true);
        l.setUint32(18, data.length, true);
        l.setUint32(22, data.length, true);
        l.setUint16(26, nameBytes.length, true);
        l.setUint16(28, 0, true);
        local.set(nameBytes, 30);

        const header = new Uint8Array(46 + nameBytes.length);
        const c = new DataView(header.buffer);
        c.setUint32(0, 0x0201_4b50, true);
        c.setUint16(4, VERSION, true);
        c.setUint16(6, VERSION, true);
        c.setUint16(8, UTF8_NAMES, true);
        c.setUint16(10, 0, true);
        c.setUint16(12, time, true);
        c.setUint16(14, day, true);
        c.setUint32(16, crc, true);
        c.setUint32(20, data.length, true);
        c.setUint32(24, data.length, true);
        c.setUint16(28, nameBytes.length, true);
        // Extra length, comment length, disk number, and attributes stay zero.
        c.setUint32(42, offset, true);
        header.set(nameBytes, 46);

        parts.push(local, data);
        central.push(header);
        offset += local.length + data.length;
    }

    const centralSize = central.reduce((sum, header) => sum + header.length, 0);
    const end = new Uint8Array(22);
    const e = new DataView(end.buffer);
    e.setUint32(0, 0x0605_4b50, true);
    e.setUint16(8, entries.length, true);
    e.setUint16(10, entries.length, true);
    e.setUint32(12, centralSize, true);
    e.setUint32(16, offset, true);
    parts.push(...central, end);
    return parts;
}
