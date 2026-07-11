package com.github.alexeylapin.pinveil.config;

import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;
import java.util.List;

/**
 * Policy for stored messages: lifetimes, payload limits, and server capacity.
 * The {@code pbkdf2Iterations} value is the client-side KDF cost surfaced to the browser.
 */
@ConfigurationProperties("app.messages")
public class MessagePolicyConfig {

    private Duration minTtl = Duration.ofMinutes(1);
    private Duration maxTtl = Duration.ofHours(24);
    private Duration defaultTtl = Duration.ofMinutes(15);
    private List<Duration> ttlPresets = List.of(Duration.ofMinutes(15), Duration.ofMinutes(30), Duration.ofHours(1));
    private long maxPayloadBytes = 25L * 1024 * 1024;
    private int pbkdf2Iterations = 600_000;

    public Duration getMinTtl() {
        return minTtl;
    }

    public void setMinTtl(Duration minTtl) {
        this.minTtl = minTtl;
    }

    public Duration getMaxTtl() {
        return maxTtl;
    }

    public void setMaxTtl(Duration maxTtl) {
        this.maxTtl = maxTtl;
    }

    public Duration getDefaultTtl() {
        return defaultTtl;
    }

    public void setDefaultTtl(Duration defaultTtl) {
        this.defaultTtl = defaultTtl;
    }

    public List<Duration> getTtlPresets() {
        return ttlPresets;
    }

    public void setTtlPresets(List<Duration> ttlPresets) {
        this.ttlPresets = ttlPresets;
    }

    public long getMaxPayloadBytes() {
        return maxPayloadBytes;
    }

    public void setMaxPayloadBytes(long maxPayloadBytes) {
        this.maxPayloadBytes = maxPayloadBytes;
    }

    public int getPbkdf2Iterations() {
        return pbkdf2Iterations;
    }

    public void setPbkdf2Iterations(int pbkdf2Iterations) {
        this.pbkdf2Iterations = pbkdf2Iterations;
    }

}
