package com.github.alexeylapin.pinveil.config;

import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Capacity limits specific to the in-memory message store, guarding heap usage.
 * These are properties of that backend, not of the storage abstraction: a
 * different backend (e.g. relational) would define its own settings, if any.
 */
@ConfigurationProperties("app.store.in-memory")
public class InMemoryStoreConfig {

    private int maxMessages = 1000;
    private long maxBytes = 256L * 1024 * 1024;

    public int getMaxMessages() {
        return maxMessages;
    }

    public void setMaxMessages(int maxMessages) {
        this.maxMessages = maxMessages;
    }

    public long getMaxBytes() {
        return maxBytes;
    }

    public void setMaxBytes(long maxBytes) {
        this.maxBytes = maxBytes;
    }

}
