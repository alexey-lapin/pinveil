package com.github.alexeylapin.pinveil.message;

/**
 * Domain failure carrying a {@link MessageError} category for the web layer to translate.
 */
public final class MessageException extends RuntimeException {

    private final MessageError error;

    public MessageException(MessageError error, String message) {
        super(message);
        this.error = error;
    }

    public MessageError error() {
        return error;
    }

}
