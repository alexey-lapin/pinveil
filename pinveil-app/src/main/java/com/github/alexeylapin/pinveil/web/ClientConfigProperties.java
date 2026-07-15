package com.github.alexeylapin.pinveil.web;

import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;
import java.util.List;

/**
 * Binds the {@code app.messages.*} keys the browser needs but the server never
 * enforces: which TTLs the UI offers, which one it preselects, and the client-side
 * KDF cost. The limits the server does enforce bind onto {@code MessagePolicyProperties}.
 */
@ConfigurationProperties("app.messages")
public class ClientConfigProperties {

    private Duration defaultTtl = Duration.ofMinutes(15);
    private List<Duration> ttlPresets = List.of(Duration.ofMinutes(15), Duration.ofMinutes(30), Duration.ofHours(1));
    private int pbkdf2Iterations = 600_000;

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

    public int getPbkdf2Iterations() {
        return pbkdf2Iterations;
    }

    public void setPbkdf2Iterations(int pbkdf2Iterations) {
        this.pbkdf2Iterations = pbkdf2Iterations;
    }

}
