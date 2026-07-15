package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.message.MessageStore;
import io.micronaut.scheduling.annotation.Scheduled;
import jakarta.inject.Singleton;

import java.time.Instant;

/**
 * Periodically evicts expired messages from the store. Scheduling is a runtime
 * concern, so it lives in the app rather than the framework-free core.
 */
@Singleton
public class ExpiredMessageSweeper {

    private final MessageStore store;

    public ExpiredMessageSweeper(MessageStore store) {
        this.store = store;
    }

    @Scheduled(fixedDelay = "1m")
    void sweep() {
        store.removeExpired(Instant.now());
    }

}
