package com.github.alexeylapin.pinveil.message;

/**
 * Request to store an encrypted blob under a PIN for a bounded lifetime.
 */
public record CreateCommand(
        byte[] blob,
        String pin,
        long ttlSeconds
) {

}
