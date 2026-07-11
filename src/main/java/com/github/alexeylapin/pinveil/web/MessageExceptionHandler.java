package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.message.MessageException;
import com.github.alexeylapin.pinveil.web.dto.ErrorResponse;
import io.micronaut.http.HttpRequest;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.HttpStatus;
import io.micronaut.http.annotation.Produces;
import io.micronaut.http.server.exceptions.ExceptionHandler;
import jakarta.inject.Singleton;

/**
 * Maps {@link MessageException} categories to HTTP responses. Not-found and
 * forbidden cases return deliberately generic messages so they reveal nothing
 * about whether a message exists.
 */
@Produces
@Singleton
public class MessageExceptionHandler implements ExceptionHandler<MessageException, HttpResponse<ErrorResponse>> {

    @Override
    public HttpResponse<ErrorResponse> handle(HttpRequest request, MessageException exception) {
        return switch (exception.error()) {
            case INVALID_REQUEST -> status(HttpStatus.BAD_REQUEST, exception.getMessage());
            case PAYLOAD_TOO_LARGE -> status(HttpStatus.REQUEST_ENTITY_TOO_LARGE, exception.getMessage());
            case CAPACITY_REACHED -> status(HttpStatus.SERVICE_UNAVAILABLE, exception.getMessage());
            case FORBIDDEN -> status(HttpStatus.FORBIDDEN, "Unable to retrieve message");
            case NOT_FOUND -> status(HttpStatus.NOT_FOUND, "Message unavailable");
        };
    }

    private static HttpResponse<ErrorResponse> status(HttpStatus status, String message) {
        return HttpResponse.status(status).body(new ErrorResponse(message));
    }

}
