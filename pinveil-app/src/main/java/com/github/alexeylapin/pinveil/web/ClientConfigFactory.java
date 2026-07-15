package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.message.MessagePolicy;
import com.github.alexeylapin.pinveil.web.dto.ClientConfig;
import jakarta.inject.Singleton;

import java.time.Duration;
import java.util.List;

/**
 * Builds the browser-facing {@link ClientConfig}, including human-readable labels for
 * the TTL presets. It draws the advertised limits from the same {@link MessagePolicy}
 * the core enforces, so the UI cannot offer something the server would reject.
 */
@Singleton
public class ClientConfigFactory {

    private final MessagePolicy policy;
    private final ClientConfigProperties properties;

    public ClientConfigFactory(MessagePolicy policy, ClientConfigProperties properties) {
        this.policy = policy;
        this.properties = properties;
    }

    public ClientConfig create() {
        List<ClientConfig.TtlPreset> presets = properties.getTtlPresets().stream()
                .map(duration -> new ClientConfig.TtlPreset(duration.toSeconds(), label(duration)))
                .toList();

        return new ClientConfig(
                policy.maxPayloadBytes(),
                properties.getPbkdf2Iterations(),
                policy.minTtl().toSeconds(),
                policy.maxTtl().toSeconds(),
                properties.getDefaultTtl().toSeconds(),
                presets
        );
    }

    private static String label(Duration duration) {
        long seconds = duration.toSeconds();
        if (seconds % 3600 == 0) {
            long hours = seconds / 3600;
            return hours + " hour" + (hours == 1 ? "" : "s");
        }
        if (seconds % 60 == 0) {
            long minutes = seconds / 60;
            return minutes + " minute" + (minutes == 1 ? "" : "s");
        }
        return seconds + " seconds";
    }

}
