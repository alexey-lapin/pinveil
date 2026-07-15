package com.github.alexeylapin.pinveil.ratelimit;

import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

import java.time.Duration;

/**
 * Fixed-window rate-limit settings, expressed per named bucket, bound from
 * {@code app.rate-limit.*}.
 */
@ConfigurationProperties("app.rate-limit")
public interface RateLimitConfig {

    /** Bucket guarding message creation. */
    String CREATE_BUCKET = "create";
    /** Bucket guarding message retrieval. */
    String RETRIEVE_BUCKET = "retrieve";

    @Bindable(defaultValue = "1m")
    Duration getWindow();

    @Bindable(defaultValue = "10")
    int getCreateRequestsPerWindow();

    @Bindable(defaultValue = "30")
    int getRetrieveRequestsPerWindow();

    /**
     * @return the request allowance for the given bucket within one window.
     * @throws IllegalArgumentException if the bucket is unknown.
     */
    default int limitFor(String bucket) {
        return switch (bucket) {
            case CREATE_BUCKET -> getCreateRequestsPerWindow();
            case RETRIEVE_BUCKET -> getRetrieveRequestsPerWindow();
            default -> throw new IllegalArgumentException("Unknown rate-limit bucket: " + bucket);
        };
    }

}
