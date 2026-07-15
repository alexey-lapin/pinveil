package com.github.alexeylapin.pinveil.message;

/**
 * Categories of failure that can occur while creating or retrieving a message.
 * The web layer maps each to an HTTP status.
 */
public enum MessageError {
    INVALID_REQUEST,
    PAYLOAD_TOO_LARGE,
    CAPACITY_REACHED,
    FORBIDDEN,
    NOT_FOUND
}
