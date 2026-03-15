package com.example.model;

import java.time.Instant;

public class StoredMessage {
    private final String id;
    private final byte[] blob;
    private final String pinVerifier;
    private final String pinSalt;
    private final Instant expiresAt;
    private final Instant createdAt;
    private int failedPinAttempts;

    public StoredMessage(
        String id,
        byte[] blob,
        String pinVerifier,
        String pinSalt,
        Instant expiresAt,
        Instant createdAt
    ) {
        this.id = id;
        this.blob = blob;
        this.pinVerifier = pinVerifier;
        this.pinSalt = pinSalt;
        this.expiresAt = expiresAt;
        this.createdAt = createdAt;
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

    public Instant getCreatedAt() {
        return createdAt;
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
