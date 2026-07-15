package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import com.github.alexeylapin.pinveil.ratelimit.RateLimitConfig;
import com.github.alexeylapin.pinveil.web.ClientConfigProperties;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.Test;

import java.time.Duration;

import static org.junit.jupiter.api.Assertions.assertEquals;

@MicronautTest
class ConfigBindingTest {

    @Inject
    RateLimitConfig rateLimitConfig;

    @Inject
    PinSecurityProperties pinSecurityProperties;

    @Inject
    MessagePolicyProperties messagePolicyProperties;

    @Inject
    ClientConfigProperties clientConfigProperties;

    @Inject
    MessagePolicy messagePolicy;

    @Test
    void bindsOverridesFromTestConfiguration() {
        // Values come from application-test.properties.
        assertEquals(100, rateLimitConfig.getCreateRequestsPerWindow());
        assertEquals("test-pepper", pinSecurityProperties.getPepper());
    }

    @Test
    void splitsSharedMessagesPrefixAcrossBothPropertyClasses() {
        // app.messages.* binds onto two @ConfigurationProperties classes: the limits the core
        // enforces, and the values only the browser consumes. Each takes the keys it declares.
        assertEquals(Duration.ofMinutes(1), messagePolicyProperties.getMinTtl());
        assertEquals(Duration.ofHours(24), messagePolicyProperties.getMaxTtl());
        assertEquals(26_214_400L, messagePolicyProperties.getMaxPayloadBytes());

        assertEquals(Duration.ofMinutes(15), clientConfigProperties.getDefaultTtl());
        assertEquals(600_000, clientConfigProperties.getPbkdf2Iterations());
        assertEquals(3, clientConfigProperties.getTtlPresets().size());
    }

    @Test
    void assemblesMessagePolicyFromBothMessagesAndPinPrefixes() {
        assertEquals(Duration.ofMinutes(1), messagePolicy.minTtl());
        assertEquals(26_214_400L, messagePolicy.maxPayloadBytes());
        // app.pin.max-failed-attempts is retrieval policy, so it lands on MessagePolicy
        // rather than reaching the message service via the PIN configuration.
        assertEquals(3, messagePolicy.maxFailedAttempts());
    }

}
