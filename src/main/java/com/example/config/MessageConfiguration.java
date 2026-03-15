package com.example.config;

import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;
import java.util.List;

@ConfigurationProperties("app.messages")
public class MessageConfiguration {
    private Duration minTtl = Duration.ofMinutes(1);

    private Duration maxTtl = Duration.ofHours(24);

    private Duration defaultTtl = Duration.ofMinutes(15);

    private List<Duration> ttlPresets = List.of(Duration.ofMinutes(15), Duration.ofMinutes(30), Duration.ofHours(1));

    private long maxPayloadBytes = 25L * 1024 * 1024;

    private int maxFailedPinAttempts = 3;

    private int maxStoredMessages = 1000;

    private long maxStoredBytes = 256L * 1024 * 1024;

    private int pbkdf2Iterations = 600000;

    private String pinPepper = "";

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

    public int getMaxFailedPinAttempts() {
        return maxFailedPinAttempts;
    }

    public void setMaxFailedPinAttempts(int maxFailedPinAttempts) {
        this.maxFailedPinAttempts = maxFailedPinAttempts;
    }

    public int getMaxStoredMessages() {
        return maxStoredMessages;
    }

    public void setMaxStoredMessages(int maxStoredMessages) {
        this.maxStoredMessages = maxStoredMessages;
    }

    public long getMaxStoredBytes() {
        return maxStoredBytes;
    }

    public void setMaxStoredBytes(long maxStoredBytes) {
        this.maxStoredBytes = maxStoredBytes;
    }

    public int getPbkdf2Iterations() {
        return pbkdf2Iterations;
    }

    public void setPbkdf2Iterations(int pbkdf2Iterations) {
        this.pbkdf2Iterations = pbkdf2Iterations;
    }

    public String getPinPepper() {
        return pinPepper;
    }

    public void setPinPepper(String pinPepper) {
        this.pinPepper = pinPepper;
    }
}
