package com.github.alexeylapin.pinveil.message;

import java.time.Duration;

/**
 * Limits the service enforces on stored messages: the accepted TTL range, the payload
 * ceiling, and how many failed PIN attempts burn a message. Values the UI merely
 * advertises (TTL presets, the client-side KDF cost) are not policy and live in the app.
 */
public record MessagePolicy(
        Duration minTtl,
        Duration maxTtl,
        long maxPayloadBytes,
        int maxFailedAttempts
) {

}
