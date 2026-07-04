package com.github.alexeylapin.pinveil.message;

import java.time.Instant;

/**
 * Outcome of a successful create: the server-issued id and when it expires.
 */
public record CreateResult(
        String id,
        Instant expiresAt
) {

}
