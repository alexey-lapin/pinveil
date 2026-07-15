package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Get;
import io.micronaut.json.JsonMapper;
import io.micronaut.views.View;

import java.io.IOException;
import java.util.Map;

/**
 * Serves the HTML pages, embedding the client configuration as JSON produced by
 * the {@link JsonMapper} (no hand-built JSON).
 */
@Controller
public class PageController {

    private final MessagePolicyConfig policy;
    private final ClientConfigFactory clientConfigFactory;
    private final JsonMapper jsonMapper;

    public PageController(MessagePolicyConfig policy, ClientConfigFactory clientConfigFactory, JsonMapper jsonMapper) {
        this.policy = policy;
        this.clientConfigFactory = clientConfigFactory;
        this.jsonMapper = jsonMapper;
    }

    @View("index")
    @Get("/")
    public Map<String, Object> index() throws IOException {
        return Map.of(
                "clientConfigJson", clientConfigJson(),
                "defaultTtlSeconds", policy.getDefaultTtl().toSeconds()
        );
    }

    @View("message")
    @Get("/message/{id}")
    public Map<String, Object> message(String id) throws IOException {
        return Map.of(
                "messageId", id,
                "clientConfigJson", clientConfigJson()
        );
    }

    private String clientConfigJson() throws IOException {
        return jsonMapper.writeValueAsString(clientConfigFactory.create());
    }

}
