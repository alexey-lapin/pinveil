package com.example.config;

import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;

@ConfigurationProperties("app.rate-limit")
public class RateLimitConfiguration {
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
}
