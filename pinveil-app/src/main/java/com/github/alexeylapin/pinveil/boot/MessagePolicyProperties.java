package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import io.micronaut.context.annotation.ConfigurationProperties;

import java.time.Duration;

/**
 * Binds the {@code app.messages.*} limits the core enforces. The remaining
 * {@code app.messages.*} keys describe what the UI offers rather than what the server
 * enforces, and bind onto {@code ClientConfigProperties} instead.
 *
 * @see MessagePolicy
 */
@ConfigurationProperties("app.messages")
public class MessagePolicyProperties {

    private Duration minTtl = Duration.ofMinutes(1);
    private Duration maxTtl = Duration.ofHours(24);
    private long maxPayloadBytes = 25L * 1024 * 1024;

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

    public long getMaxPayloadBytes() {
        return maxPayloadBytes;
    }

    public void setMaxPayloadBytes(long maxPayloadBytes) {
        this.maxPayloadBytes = maxPayloadBytes;
    }

}
