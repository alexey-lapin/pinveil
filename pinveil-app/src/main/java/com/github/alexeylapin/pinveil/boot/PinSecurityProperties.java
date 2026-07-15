package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

/**
 * Binds {@code app.pin.*}: the secret pepper, Argon2 cost parameters, and how many
 * failed attempts burn down a message. {@code DomainFactory} splits these between the
 * PIN verifier (pepper and cost) and the message policy (failed attempts), so that
 * only the verifier ever sees the pepper.
 */
@ConfigurationProperties("app.pin")
public interface PinSecurityProperties {

    /** Empty when unset: the pepper is optional, and an absent one must not hash as "null". */
    @Bindable(defaultValue = "")
    String getPepper();

    @Bindable(defaultValue = "3")
    int getMaxFailedAttempts();

    @Bindable(defaultValue = "3")
    int getArgon2Iterations();

    /** Defaults to the OWASP recommendation for Argon2id. */
    @Bindable(defaultValue = "19456")
    int getArgon2MemoryKib();

    @Bindable(defaultValue = "1")
    int getArgon2Parallelism();

}
