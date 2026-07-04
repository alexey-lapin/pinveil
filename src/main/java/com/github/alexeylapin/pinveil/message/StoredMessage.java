package com.github.alexeylapin.pinveil.message;

import java.time.Instant;

/**
 * A stored ciphertext blob with its PIN verifier and expiry. The failed-attempt
 * counter is the only mutable state; everything else is fixed at creation.
 */
public final class StoredMessage {

    private final String id;
    private final byte[] blob;
    private final String pinVerifier;
    private final String pinSalt;
    private final Instant expiresAt;
    private int failedPinAttempts;

    public StoredMessage(String id, byte[] blob, String pinVerifier, String pinSalt, Instant expiresAt) {
        this.id = id;
        this.blob = blob;
        this.pinVerifier = pinVerifier;
        this.pinSalt = pinSalt;
        this.expiresAt = expiresAt;
        this.failedPinAttempts = 0;
    }

    public String getId() {
        return id;
    }

    public byte[] getBlob() {
        return blob;
    }

    public String getPinVerifier() {
        return pinVerifier;
    }

    public String getPinSalt() {
        return pinSalt;
    }

    public Instant getExpiresAt() {
        return expiresAt;
    }

    public boolean isExpiredAt(Instant now) {
        return !expiresAt.isAfter(now);
    }

    public int getFailedPinAttempts() {
        return failedPinAttempts;
    }

    public void incrementFailedPinAttempts() {
        failedPinAttempts++;
    }

    public long sizeInBytes() {
        return blob.length;
    }

}
