package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

import java.time.Duration;

/**
 * Binds the {@code app.messages.*} limits the core enforces. The remaining
 * {@code app.messages.*} keys describe what the UI offers rather than what the server
 * enforces, and bind onto {@code ClientConfigProperties} instead.
 *
 * @see MessagePolicy
 */
@ConfigurationProperties("app.messages")
public interface MessagePolicyProperties {

    @Bindable(defaultValue = "1m")
    Duration getMinTtl();

    @Bindable(defaultValue = "24h")
    Duration getMaxTtl();

    /** Defaults to 25 MiB. */
    @Bindable(defaultValue = "26214400")
    long getMaxPayloadBytes();

}
