package com.github.alexeylapin.pinveil.web.dto;

import io.micronaut.serde.annotation.Serdeable;

@Serdeable
public record ErrorResponse(String error) {

}
