package com.github.alexeylapin.pinveil.security;

/**
 * Argon2id cost parameters, plus the server-side pepper mixed into every PIN hash.
 * Defaults for these live with the application's configuration; the memory cost
 * follows the OWASP recommendation for Argon2id.
 */
public record Argon2Params(
        String pepper,
        int iterations,
        int memoryKib,
        int parallelism
) {
}
