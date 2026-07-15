package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import com.github.alexeylapin.pinveil.web.dto.ClientConfig;
import jakarta.inject.Singleton;

import java.time.Duration;
import java.util.List;

/**
 * Builds the browser-facing {@link ClientConfig} from server policy, including
 * human-readable labels for the TTL presets.
 */
@Singleton
public class ClientConfigFactory {

    private final MessagePolicyConfig policy;

    public ClientConfigFactory(MessagePolicyConfig policy) {
        this.policy = policy;
    }

    public ClientConfig create() {
        List<ClientConfig.TtlPreset> presets = policy.getTtlPresets().stream()
                .map(duration -> new ClientConfig.TtlPreset(duration.toSeconds(), label(duration)))
                .toList();

        return new ClientConfig(
                policy.getMaxPayloadBytes(),
                policy.getPbkdf2Iterations(),
                policy.getMinTtl().toSeconds(),
                policy.getMaxTtl().toSeconds(),
                policy.getDefaultTtl().toSeconds(),
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
