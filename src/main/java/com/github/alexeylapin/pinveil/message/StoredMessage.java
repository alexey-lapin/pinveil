package com.github.alexeylapin.pinveil.message;

import java.time.Instant;

/**
 * An immutable stored ciphertext blob with its PIN verifier, expiry, and the
 * number of failed PIN attempts recorded so far.
 *
 * <p>State transitions produce a new instance rather than mutating in place, so
 * the value is safe to share and behaves correctly with stores that return
 * detached copies: recording a failed attempt yields a new {@link StoredMessage}
 * that the caller must save back.
 */
public record StoredMessage(
        String id,
        byte[] blob,
        String pinVerifier,
        String pinSalt,
        Instant expiresAt,
        int failedPinAttempts
) {

    /** Creates a message with no failed attempts recorded yet. */
    public StoredMessage(String id, byte[] blob, String pinVerifier, String pinSalt, Instant expiresAt) {
        this(id, blob, pinVerifier, pinSalt, expiresAt, 0);
    }

    /**
     * @return a copy of this message with the failed-attempt count increased by one.
     */
    public StoredMessage withIncrementedFailedAttempts() {
        return new StoredMessage(id, blob, pinVerifier, pinSalt, expiresAt, failedPinAttempts + 1);
    }

    public boolean isExpiredAt(Instant now) {
        return !expiresAt.isAfter(now);
    }

    public long sizeInBytes() {
        return blob.length;
    }

}
