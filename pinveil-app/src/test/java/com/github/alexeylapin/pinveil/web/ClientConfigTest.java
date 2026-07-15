package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import com.github.alexeylapin.pinveil.web.dto.ClientConfig;
import io.micronaut.json.JsonMapper;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.time.Duration;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

@MicronautTest
class ClientConfigTest {

    @Inject
    JsonMapper jsonMapper;

    @Test
    void buildsHumanReadableTtlLabels() {
        MessagePolicy policy = new MessagePolicy(Duration.ofMinutes(1), Duration.ofHours(24), 1024, 3);
        ClientConfigProperties properties =
                clientConfig(List.of(Duration.ofMinutes(15), Duration.ofMinutes(30), Duration.ofHours(1)));

        ClientConfig config = new ClientConfigFactory(policy, properties).create();

        assertEquals(
                List.of("15 minutes", "30 minutes", "1 hour"),
                config.ttlPresets().stream().map(ClientConfig.TtlPreset::label).toList()
        );
        assertEquals(900, config.ttlPresets().get(0).seconds());
    }

    @Test
    void serializesWithClientFacingFieldNames() throws IOException {
        ClientConfig config = new ClientConfig(
                1024, 600_000, 60, 86_400, 900,
                List.of(new ClientConfig.TtlPreset(900, "15 minutes"))
        );

        String json = jsonMapper.writeValueAsString(config);

        assertTrue(json.contains("\"maxPayloadBytes\":1024"), json);
        assertTrue(json.contains("\"pbkdf2Iterations\":600000"), json);
        assertTrue(json.contains("\"defaultTtlSeconds\":900"), json);
        assertTrue(json.contains("\"label\":\"15 minutes\""), json);
    }

    /** Stands in for the Micronaut-generated binding of {@link ClientConfigProperties}. */
    private static ClientConfigProperties clientConfig(List<Duration> presets) {
        return new ClientConfigProperties() {

            @Override
            public Duration getDefaultTtl() {
                return Duration.ofMinutes(15);
            }

            @Override
            public List<Duration> getTtlPresets() {
                return presets;
            }

            @Override
            public int getPbkdf2Iterations() {
                return 600_000;
            }

        };
    }

}
