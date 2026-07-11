package com.github.alexeylapin.pinveil.web.dto;

import io.micronaut.serde.annotation.Serdeable;

import java.util.List;

/**
 * Configuration handed to the browser as JSON. Field names match the keys the
 * client reads from the {@code app-config} script element.
 */
@Serdeable
public record ClientConfig(
        long maxPayloadBytes,
        int pbkdf2Iterations,
        long minTtlSeconds,
        long maxTtlSeconds,
        long defaultTtlSeconds,
        List<TtlPreset> ttlPresets
) {

    @Serdeable
    public record TtlPreset(long seconds, String label) {

    }

}
