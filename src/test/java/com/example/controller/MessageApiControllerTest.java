package com.example.controller;

import io.micronaut.http.HttpRequest;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.MediaType;
import io.micronaut.http.client.HttpClient;
import io.micronaut.http.client.annotation.Client;
import io.micronaut.http.client.exceptions.HttpClientResponseException;
import io.micronaut.http.client.multipart.MultipartBody;
import io.micronaut.json.tree.JsonNode;
import io.micronaut.test.extensions.junit5.annotation.MicronautTest;
import jakarta.inject.Inject;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

@MicronautTest
class MessageApiControllerTest {
    @Inject
    @Client("/")
    HttpClient client;

    @Test
    void createsRetrievesAndDeletesBlobPackage() {
        MultipartBody body = MultipartBody.builder()
            .addPart("blob", "blob.bin", MediaType.APPLICATION_OCTET_STREAM_TYPE, "opaque-blob-content".getBytes())
            .addPart("pin", "654321")
            .addPart("ttl", "60")
            .build();

        HttpResponse<JsonNode> createResponse = client.toBlocking().exchange(
            HttpRequest.POST("/api/messages", body).contentType(MediaType.MULTIPART_FORM_DATA_TYPE),
            JsonNode.class
        );

        assertEquals(201, createResponse.code());
        assertNotNull(createResponse.body().get("id"));
        assertNotNull(createResponse.body().get("expiresAt"));

        String messageId = createResponse.body().get("id").getStringValue();
        assertTrue(messageId.matches("^[a-z]+-[a-z]+-[a-z]+-\\d{6}$"));

        HttpResponse<byte[]> retrieveResponse = client.toBlocking().exchange(
            HttpRequest.POST("/api/messages/" + messageId + "/retrieve", "{\"pin\":\"654321\"}")
                .contentType(MediaType.APPLICATION_JSON_TYPE),
            byte[].class
        );

        assertEquals(200, retrieveResponse.code());
        assertNotNull(retrieveResponse.body());
        assertTrue(retrieveResponse.body().length > 0);

        HttpClientResponseException exception = assertThrows(
            HttpClientResponseException.class,
            () -> client.toBlocking().exchange(
                HttpRequest.POST("/api/messages/" + messageId + "/retrieve", "{\"pin\":\"654321\"}")
                    .contentType(MediaType.APPLICATION_JSON_TYPE),
                byte[].class
            )
        );

        assertEquals(404, exception.getStatus().getCode());
    }
}
