package com.github.alexeylapin.pinveil.web;

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

    private final ClientConfigProperties properties;
    private final ClientConfigFactory clientConfigFactory;
    private final JsonMapper jsonMapper;

    public PageController(ClientConfigProperties properties, ClientConfigFactory clientConfigFactory, JsonMapper jsonMapper) {
        this.properties = properties;
        this.clientConfigFactory = clientConfigFactory;
        this.jsonMapper = jsonMapper;
    }

    @View("index")
    @Get("/")
    public Map<String, Object> index() throws IOException {
        return Map.of(
                "clientConfigJson", clientConfigJson(),
                "defaultTtlSeconds", properties.getDefaultTtl().toSeconds()
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
