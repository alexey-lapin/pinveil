package com.example.controller;

import com.example.config.MessageConfiguration;
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Get;
import io.micronaut.views.View;
import jakarta.inject.Singleton;

import java.time.Duration;
import java.util.List;
import java.util.Map;

@Controller
public class PageController {
    private final MessageConfiguration configuration;

    public PageController(MessageConfiguration configuration) {
        this.configuration = configuration;
    }

    @View("index")
    @Get("/")
    public Map<String, Object> index() {
        return Map.of(
            "clientConfigJson", clientConfigJson(),
            "defaultTtlSeconds", configuration.getDefaultTtl().toSeconds()
        );
    }

    @View("message")
    @Get("/message/{id}")
    public Map<String, Object> message(String id) {
        return Map.of(
            "messageId", id,
            "clientConfigJson", clientConfigJson()
        );
    }

    private String clientConfigJson() {
        String ttlPresets = configuration.getTtlPresets().stream()
            .map(this::ttlPresetJson)
            .reduce((left, right) -> left + "," + right)
            .orElse("");

        return "{" +
            "\"maxPayloadBytes\":" + configuration.getMaxPayloadBytes() + "," +
            "\"pbkdf2Iterations\":" + configuration.getPbkdf2Iterations() + "," +
            "\"minTtlSeconds\":" + configuration.getMinTtl().toSeconds() + "," +
            "\"maxTtlSeconds\":" + configuration.getMaxTtl().toSeconds() + "," +
            "\"defaultTtlSeconds\":" + configuration.getDefaultTtl().toSeconds() + "," +
            "\"ttlPresets\": [" + ttlPresets + "]" +
            "}";
    }

    private String ttlPresetJson(Duration duration) {
        long seconds = duration.toSeconds();
        return "{" +
            "\"seconds\":" + seconds + "," +
            "\"label\":\"" + ttlLabel(duration) + "\"" +
            "}";
    }

    private String ttlLabel(Duration duration) {
        if (duration.toHours() >= 1 && duration.toHoursPart() == 0 && duration.toMinutesPart() == 0) {
            return duration.toHours() + " hour" + (duration.toHours() == 1 ? "" : "s");
        }
        if (duration.toMinutes() >= 1 && duration.toSecondsPart() == 0) {
            return duration.toMinutes() + " minute" + (duration.toMinutes() == 1 ? "" : "s");
        }
        return duration.toSeconds() + " seconds";
    }
}
