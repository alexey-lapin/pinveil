/**
 * Page logic that doesn't touch the DOM, so the Node runner can test it: the checks before an
 * upload, expiry input, and the user-facing wording for errors and notices. Every limit comes
 * from the client config the server embeds (APP.md, "Client config"), never from constants.
 */

import { blobLength } from "./envelope.js";
import { measurePayload } from "./payload.js";
import { MAX_MIME_TYPE_BYTES, PinveilError } from "./protocol.js";
import { formatBytes } from "./ui.js";

export const DEFAULT_MIME_TYPE = "application/octet-stream";

const encoder = new TextEncoder();

/** The creation limits the server advertises. */
export function creationLimits(config) {
    return { maxFiles: config.maxFiles, maxManifestBytes: config.maxManifestBytes };
}

/**
 * The MIME type to record for a picked file. Browsers report "" for unknown types, and the
 * value is only a hint, so anything unusable becomes `application/octet-stream`.
 */
export function mimeTypeOf(file) {
    const type = file.type ?? "";
    const length = encoder.encode(type).length;
    return type.isWellFormed() && length >= 1 && length <= MAX_MIME_TYPE_BYTES ? type : DEFAULT_MIME_TYPE;
}

/** Empty text means no text part. */
export function textPart(text) {
    return text === "" ? null : text;
}

/**
 * Checks a message from its text and the files' metadata only, before any file is read.
 * Returns the exact size of the encrypted blob. Throws an `Error` worded for the sender.
 */
export function checkMessage({ text, files }, config) {
    const described = files.map((file) => ({ name: file.name, mimeType: mimeTypeOf(file), byteLength: file.size }));
    let length;
    try {
        length = measurePayload({ text: textPart(text), files: described }, creationLimits(config));
    } catch (error) {
        throw new Error(createErrorText(error, config));
    }
    const size = blobLength(length, config.pbkdf2Iterations);
    if (size > config.maxPayloadBytes) {
        throw new Error(
            `This message is ${formatBytes(size)} once encrypted. This server accepts up to ${formatBytes(config.maxPayloadBytes)}.`,
        );
    }
    return size;
}

/** Sender-facing text for a `PinveilError` raised while building a message. */
export function createErrorText(error, config) {
    if (!(error instanceof PinveilError)) {
        return error?.message || "Could not create the message.";
    }
    switch (error.reason) {
        case "empty":
            return "Add some text or at least one file.";
        case "too-many-files":
            return `This server accepts up to ${config.maxFiles} files per message.`;
        case "manifest-too-large":
            return "The file names take up too much space. Use fewer files or shorter names.";
        case "bad-name":
            return "A file name is longer than 255 bytes. Rename the file and try again.";
        case "invalid-text":
        case "malformed":
            return "The text or a file name contains characters that can't be encoded.";
        default:
            return error.message;
    }
}

/** The server's label for a TTL (APP.md, "Client config"), for a default that isn't a preset. */
export function ttlLabel(seconds) {
    const plural = (n, unit) => `${n} ${unit}${n === 1 ? "" : "s"}`;
    if (seconds % 3600 === 0) {
        return plural(seconds / 3600, "hour");
    }
    if (seconds % 60 === 0) {
        return plural(seconds / 60, "minute");
    }
    return plural(seconds, "second");
}

/**
 * The expiry choices, shortest first: every preset, plus the default when it isn't one of
 * them, so the advertised default is always selectable even with no presets at all.
 */
export function ttlChoices(config) {
    const choices = config.ttlPresets.map(({ seconds, label }) => ({ seconds, label }));
    if (!choices.some((choice) => choice.seconds === config.defaultTtlSeconds)) {
        choices.push({ seconds: config.defaultTtlSeconds, label: ttlLabel(config.defaultTtlSeconds) });
    }
    return choices.sort((a, b) => a.seconds - b.seconds);
}

/**
 * The Custom expiry range in whole minutes, inside the server's TTL range, or `null` when no
 * whole minute fits it.
 */
export function customTtlMinutes(config) {
    const min = Math.max(1, Math.ceil(config.minTtlSeconds / 60));
    const max = Math.floor(config.maxTtlSeconds / 60);
    return min <= max ? { min, max } : null;
}

/** Seconds for a Custom expiry typed in minutes. Throws an `Error` worded for the sender. */
export function ttlFromMinutes(value, config) {
    const range = customTtlMinutes(config);
    const minutes = Number(value);
    if (range === null || !/^\d+$/.test(String(value).trim()) || minutes < range.min || minutes > range.max) {
        throw new Error(
            range === null
                ? "This server doesn't accept a custom expiry."
                : `Custom expiry must be a whole number of minutes from ${range.min} to ${range.max}.`,
        );
    }
    return minutes * 60;
}

/** What the store mode means for users (APP.md invariant 10). */
export function storeModeNotice(mode) {
    if (mode === "sqlite") {
        return "This server stores encrypted messages on disk, so they survive a restart. Deleted messages may stay recoverable on that disk.";
    }
    return "This server keeps messages in memory only. A server restart deletes every message.";
}

export const OPEN_WARNING =
    "Opening deletes the message from the server, even if it then can't be decrypted, for example because the link is incomplete. Keep this page open until you have saved what you need: if the connection drops after opening, the message is lost.";

/** Recipient-facing text for a failed retrieve request. */
export function retrieveErrorText(status, serverMessage) {
    switch (status) {
        case 400:
            return "The PIN must be exactly six digits.";
        case 403:
            return "Wrong PIN. Too many wrong PINs delete the message.";
        case 404:
            return "This message doesn't exist. It may have been opened already, expired, or been deleted after too many wrong PINs.";
        case 429:
            return "Too many attempts from your address. Wait a minute and try again.";
        default:
            return serverMessage || "The server couldn't return the message. Try again shortly.";
    }
}

export const LOST_IN_TRANSIT =
    "The connection dropped while the message was downloading. The server has already deleted it, so it can't be opened again. Ask the sender to send a new one.";

/** Recipient-facing text when a delivered message can't be opened (APP.md, "Validation"). */
export function openErrorText(error) {
    const lost = "The server has already deleted it.";
    if (error instanceof PinveilError && error.stage === "decrypt") {
        return `Cannot open this message: the link doesn't match it. Check that you copied the whole link. ${lost}`;
    }
    return `Cannot open this message. ${lost}`;
}
