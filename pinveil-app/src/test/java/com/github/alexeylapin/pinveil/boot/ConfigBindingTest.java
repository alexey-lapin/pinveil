package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import com.github.alexeylapin.pinveil.ratelimit.RateLimitConfig;
import com.github.alexeylapin.pinveil.web.ClientConfigProperties;
import io.micronaut.context.ApplicationContext;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.util.List;
import java.util.Map;

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

    @Test
    void appliesBindableDefaultsForKeysAbsentFromConfiguration() {
        // app.pin.argon2-* appear in no properties file, so these can only come from the
        // @Bindable defaults declared on the interface.
        assertEquals(3, pinSecurityProperties.getArgon2Iterations());
        assertEquals(19_456, pinSecurityProperties.getArgon2MemoryKib());
        assertEquals(1, pinSecurityProperties.getArgon2Parallelism());
    }

    @Test
    void resolvesDefaultMethodAgainstBoundValues() {
        // The build warns that Micronaut skips default methods on @ConfigurationProperties
        // interfaces. It still implements the interface, so limitFor runs against the bound
        // getters rather than being stubbed out to zero.
        assertEquals(100, rateLimitConfig.limitFor(RateLimitConfig.CREATE_BUCKET));
        assertEquals(100, rateLimitConfig.limitFor(RateLimitConfig.RETRIEVE_BUCKET));
    }

    @Test
    void bindsEmptyPepperAsEmptyStringRatherThanNull() {
        // An absent pepper must hash as "" — a null would silently change every PIN hash.
        try (ApplicationContext context = ApplicationContext.run(Map.of("app.pin.pepper", ""))) {
            assertEquals("", context.getBean(PinSecurityProperties.class).getPepper());
        }
    }

    @Test
    void standsAloneOnDeclaredDefaultsWithNoConfigurationFilesAtAll() {
        // Every properties file is switched off, so each value below can only come from its
        // @Bindable default. This is what a deployment that trims application.properties gets.
        try (ApplicationContext context = ApplicationContext.builder()
                .enableDefaultPropertySources(false)
                .start()) {

            assertEquals("", context.getBean(PinSecurityProperties.class).getPepper());
            assertEquals(3, context.getBean(PinSecurityProperties.class).getMaxFailedAttempts());

            MessagePolicyProperties policy = context.getBean(MessagePolicyProperties.class);
            assertEquals(Duration.ofMinutes(1), policy.getMinTtl());
            assertEquals(Duration.ofHours(24), policy.getMaxTtl());
            assertEquals(26_214_400L, policy.getMaxPayloadBytes());

            InMemoryStoreProperties store = context.getBean(InMemoryStoreProperties.class);
            assertEquals(1000, store.getMaxMessages());
            assertEquals(268_435_456L, store.getMaxBytes());

            MessageIdProperties messageId = context.getBean(MessageIdProperties.class);
            assertEquals(3, messageId.getWords());
            assertEquals(6, messageId.getDigits());

            // The comma-separated default has to survive conversion to List<Duration>.
            ClientConfigProperties client = context.getBean(ClientConfigProperties.class);
            assertEquals(Duration.ofMinutes(15), client.getDefaultTtl());
            assertEquals(600_000, client.getPbkdf2Iterations());
            assertEquals(
                    List.of(Duration.ofMinutes(15), Duration.ofMinutes(30), Duration.ofHours(1)),
                    client.getTtlPresets()
            );

            RateLimitConfig rateLimit = context.getBean(RateLimitConfig.class);
            assertEquals(Duration.ofMinutes(1), rateLimit.getWindow());
            assertEquals(10, rateLimit.limitFor(RateLimitConfig.CREATE_BUCKET));
            assertEquals(30, rateLimit.limitFor(RateLimitConfig.RETRIEVE_BUCKET));
        }
    }

}
