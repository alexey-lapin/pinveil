package com.github.alexeylapin.pinveil.config;

import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;

/**
 * Fixed-window rate-limit settings, expressed per named bucket.
 */
@ConfigurationProperties("app.rate-limit")
public class RateLimitConfig {

    /** Bucket guarding message creation. */
    public static final String CREATE_BUCKET = "create";
    /** Bucket guarding message retrieval. */
    public static final String RETRIEVE_BUCKET = "retrieve";

    private Duration window = Duration.ofMinutes(1);
    private int createRequestsPerWindow = 10;
    private int retrieveRequestsPerWindow = 30;

    public Duration getWindow() {
        return window;
    }

    public void setWindow(Duration window) {
        this.window = window;
    }

    public int getCreateRequestsPerWindow() {
        return createRequestsPerWindow;
    }

    public void setCreateRequestsPerWindow(int createRequestsPerWindow) {
        this.createRequestsPerWindow = createRequestsPerWindow;
    }

    public int getRetrieveRequestsPerWindow() {
        return retrieveRequestsPerWindow;
    }

    public void setRetrieveRequestsPerWindow(int retrieveRequestsPerWindow) {
        this.retrieveRequestsPerWindow = retrieveRequestsPerWindow;
    }

    /**
     * @return the request allowance for the given bucket within one window.
     * @throws IllegalArgumentException if the bucket is unknown.
     */
    public int limitFor(String bucket) {
        return switch (bucket) {
            case CREATE_BUCKET -> createRequestsPerWindow;
            case RETRIEVE_BUCKET -> retrieveRequestsPerWindow;
            default -> throw new IllegalArgumentException("Unknown rate-limit bucket: " + bucket);
        };
    }

}
