package com.example.controller;

import com.example.model.CreateMessageResponse;
import com.example.model.ErrorResponse;
import com.example.service.MessageService;
import com.example.service.RateLimitService;
import io.micronaut.http.HttpRequest;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.HttpStatus;
import io.micronaut.http.MediaType;
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Error;
import io.micronaut.http.annotation.Get;
import io.micronaut.http.annotation.Header;
import io.micronaut.http.annotation.Part;
import io.micronaut.http.annotation.PathVariable;
import io.micronaut.http.annotation.Post;
import io.micronaut.http.exceptions.HttpStatusException;
import io.micronaut.http.multipart.CompletedFileUpload;
import io.micronaut.http.server.util.HttpClientAddressResolver;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.IOException;

@Controller("/api/messages")
public class MessageApiController {

    private static final Logger log = LoggerFactory.getLogger(MessageApiController.class);

    private final MessageService messageService;
    private final RateLimitService rateLimitService;
    private final HttpClientAddressResolver addressResolver;

    public MessageApiController(MessageService messageService,
                                RateLimitService rateLimitService,
                                HttpClientAddressResolver addressResolver) {
        this.messageService = messageService;
        this.rateLimitService = rateLimitService;
        this.addressResolver = addressResolver;
    }

    @Post(consumes = MediaType.MULTIPART_FORM_DATA, produces = MediaType.APPLICATION_JSON)
    public HttpResponse<CreateMessageResponse> create(
            HttpRequest<?> request,
            @Part CompletedFileUpload blob,
            @Part String pin,
            @Part String ttl
    ) throws IOException {
        enforceRateLimit(request, true);
        MessageService.CreateCommand command = new MessageService.CreateCommand(
                blob.getBytes(),
                pin,
                Long.parseLong(ttl)
        );
        MessageService.CreateResult result = messageService.create(command);
        return HttpResponse.created(new CreateMessageResponse(result.id(), result.expiresAt()));
    }

    @Get(uri = "/{id}/retrieve", produces = MediaType.APPLICATION_OCTET_STREAM)
    public HttpResponse<byte[]> retrieve(
            HttpRequest<?> request,
            @PathVariable String id,
            @Header("X-Message-Pin") String pin
    ) {
        enforceRateLimit(request, false);
        byte[] blob = messageService.retrieve(id, pin);
        return HttpResponse.ok(blob).contentType(MediaType.APPLICATION_OCTET_STREAM_TYPE);
    }

    @Error(global = true)
    public HttpResponse<ErrorResponse> onMessageException(MessageService.MessageException exception) {
        return switch (exception.error()) {
            case INVALID_REQUEST ->
                    HttpResponse.status(HttpStatus.BAD_REQUEST).body(new ErrorResponse(exception.getMessage()));
            case PAYLOAD_TOO_LARGE ->
                    HttpResponse.status(HttpStatus.valueOf(413)).body(new ErrorResponse(exception.getMessage()));
            case CAPACITY_REACHED ->
                    HttpResponse.status(HttpStatus.SERVICE_UNAVAILABLE).body(new ErrorResponse(exception.getMessage()));
            case FORBIDDEN ->
                    HttpResponse.status(HttpStatus.FORBIDDEN).body(new ErrorResponse("Unable to retrieve message"));
            case NOT_FOUND -> HttpResponse.status(HttpStatus.NOT_FOUND).body(new ErrorResponse("Message unavailable"));
        };
    }

    @Error(global = true)
    public HttpResponse<ErrorResponse> onStatusException(HttpStatusException exception) {
        return HttpResponse.status(exception.getStatus()).body(new ErrorResponse(exception.getMessage()));
    }

    @Error(global = true)
    public HttpResponse<ErrorResponse> onIllegalArgument(IllegalArgumentException exception) {
        return HttpResponse.badRequest(new ErrorResponse(exception.getMessage()));
    }

    @Error(global = true)
    public HttpResponse<ErrorResponse> onIo(IOException exception) {
        return HttpResponse.badRequest(new ErrorResponse("Invalid request payload"));
    }

    private void enforceRateLimit(HttpRequest<?> request, boolean create) {
        String key = request.getRemoteAddress().getAddress().getHostAddress();
        var resolve = addressResolver.resolve(request);
        log.info("Resolved client address {} to {}", key, resolve);
        boolean allowed = create ? rateLimitService.allowCreate(key) : rateLimitService.allowRetrieve(key);
        if (!allowed) {
            throw new HttpStatusException(HttpStatus.TOO_MANY_REQUESTS, "Rate limit exceeded");
        }
    }

}
