package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.store.in-memory.*}: capacity limits guarding the in-memory store's
 * heap usage. These are properties of that backend, not of the storage abstraction —
 * a different backend (e.g. relational) would define its own settings, if any.
 */
@ConfigurationProperties("app.store.in-memory")
public class InMemoryStoreProperties {

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
