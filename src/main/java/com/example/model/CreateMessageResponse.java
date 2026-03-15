package com.example.model;

import io.micronaut.serde.annotation.Serdeable;

import java.time.Instant;

@Serdeable
public record CreateMessageResponse(String id, Instant expiresAt) {
}
