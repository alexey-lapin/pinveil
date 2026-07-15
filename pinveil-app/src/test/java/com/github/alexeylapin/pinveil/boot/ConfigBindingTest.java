package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import com.github.alexeylapin.pinveil.config.RateLimitConfig;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

@MicronautTest
class ConfigBindingTest {

    @Inject
    RateLimitConfig rateLimitConfig;

    @Inject
    PinSecurityConfig pinSecurityConfig;

    @Test
    void bindsPropertiesOntoInheritedCoreConfigSetters() {
        // Values come from application-test.properties; proves the app @ConfigurationProperties
        // subclasses bind onto the framework-free core config classes they extend.
        assertEquals(100, rateLimitConfig.getCreateRequestsPerWindow());
        assertEquals("test-pepper", pinSecurityConfig.getPepper());
    }

}
