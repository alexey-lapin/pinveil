package com.github.alexeylapin.pinveil.message;

/**
 * Signals that a {@link MessageStore} operation could not be completed, for
 * example because the backend is full or unavailable.
 *
 * <p>Backends throw this to report storage-layer failures without exposing their
 * implementation details; the domain layer translates it into an appropriate
 * {@link MessageError}.
 */
public class MessageStoreException extends RuntimeException {

    public MessageStoreException(String message) {
        super(message);
    }

    public MessageStoreException(String message, Throwable cause) {
        super(message, cause);
    }

}
