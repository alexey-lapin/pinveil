const raw = document.getElementById("app-config")?.textContent ?? "{}";

/** Server-provided client configuration (limits, KDF cost, TTL presets). */
export const config = JSON.parse(raw);

/** A PIN is exactly six digits. */
export const PIN_PATTERN = /^\d{6}$/;

/** Upper bound for the custom expiry input, in minutes (24 hours). */
export const MAX_CUSTOM_TTL_MINUTES = 1440;

export function validatePin(pin) {
    if (!PIN_PATTERN.test(pin)) {
        throw new Error("PIN must be exactly six digits.");
    }
}

/** Maximum payload size in whole megabytes, for user-facing messages. */
export function maxPayloadMegabytes() {
    return Math.round(config.maxPayloadBytes / (1024 * 1024));
}
