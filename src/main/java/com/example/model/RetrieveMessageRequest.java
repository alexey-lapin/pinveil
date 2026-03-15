package com.example.model;

import io.micronaut.serde.annotation.Serdeable;

@Serdeable
public record RetrieveMessageRequest(String pin) {
}
