package com.github.alexeylapin.pinveil.web;

import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

import java.time.Duration;
import java.util.List;

/**
 * Binds the {@code app.messages.*} keys the browser needs but the server never
 * enforces: which TTLs the UI offers, which one it preselects, and the client-side
 * KDF cost. The limits the server does enforce bind onto {@code MessagePolicyProperties}.
 */
@ConfigurationProperties("app.messages")
public interface ClientConfigProperties {

    @Bindable(defaultValue = "15m")
    Duration getDefaultTtl();

    @Bindable(defaultValue = "15m,30m,1h")
    List<Duration> getTtlPresets();

    @Bindable(defaultValue = "600000")
    int getPbkdf2Iterations();

}
