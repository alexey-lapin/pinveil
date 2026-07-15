package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.pin.*}: the secret pepper, Argon2 cost parameters, and how many
 * failed attempts burn down a message. {@code DomainFactory} splits these between the
 * PIN verifier (pepper and cost) and the message policy (failed attempts), so that
 * only the verifier ever sees the pepper.
 */
@ConfigurationProperties("app.pin")
public class PinSecurityProperties {

    private String pepper = "";
    private int maxFailedAttempts = 3;
    private int argon2Iterations = 3;
    private int argon2MemoryKib = 19_456;
    private int argon2Parallelism = 1;

    public String getPepper() {
        return pepper;
    }

    public void setPepper(String pepper) {
        this.pepper = pepper;
    }

    public int getMaxFailedAttempts() {
        return maxFailedAttempts;
    }

    public void setMaxFailedAttempts(int maxFailedAttempts) {
        this.maxFailedAttempts = maxFailedAttempts;
    }

    public int getArgon2Iterations() {
        return argon2Iterations;
    }

    public void setArgon2Iterations(int argon2Iterations) {
        this.argon2Iterations = argon2Iterations;
    }

    public int getArgon2MemoryKib() {
        return argon2MemoryKib;
    }

    public void setArgon2MemoryKib(int argon2MemoryKib) {
        this.argon2MemoryKib = argon2MemoryKib;
    }

    public int getArgon2Parallelism() {
        return argon2Parallelism;
    }

    public void setArgon2Parallelism(int argon2Parallelism) {
        this.argon2Parallelism = argon2Parallelism;
    }

}
