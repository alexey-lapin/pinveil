package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

/**
 * Binds {@code app.store.in-memory.*}: capacity limits guarding the in-memory store's
 * heap usage. These are properties of that backend, not of the storage abstraction —
 * a different backend (e.g. relational) would define its own settings, if any.
 */
@ConfigurationProperties("app.store.in-memory")
public interface InMemoryStoreProperties {

    @Bindable(defaultValue = "1000")
    int getMaxMessages();

    /** Defaults to 256 MiB. */
    @Bindable(defaultValue = "268435456")
    long getMaxBytes();

}
