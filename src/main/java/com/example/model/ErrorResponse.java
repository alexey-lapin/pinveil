package com.example.model;

import io.micronaut.serde.annotation.Serdeable;

@Serdeable
public record ErrorResponse(String error) {
}
