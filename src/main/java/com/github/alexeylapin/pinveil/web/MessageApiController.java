package com.github.alexeylapin.pinveil.web;

import com.github.alexeylapin.pinveil.message.CreateCommand;
import com.github.alexeylapin.pinveil.message.CreateResult;
import com.github.alexeylapin.pinveil.message.MessageError;
import com.github.alexeylapin.pinveil.message.MessageException;
import com.github.alexeylapin.pinveil.message.MessageService;
import com.github.alexeylapin.pinveil.web.dto.CreateMessageResponse;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.MediaType;
import io.micronaut.http.annotation.Controller;
import io.micronaut.http.annotation.Get;
import io.micronaut.http.annotation.Header;
import io.micronaut.http.annotation.Part;
import io.micronaut.http.annotation.PathVariable;
import io.micronaut.http.annotation.Post;
import io.micronaut.http.multipart.CompletedFileUpload;

import java.io.IOException;

/**
 * HTTP edge for the message API. Translates the multipart request into a domain
 * command and delegates everything else; rate limiting is handled upstream by a
 * filter and error mapping by {@link MessageExceptionHandler}.
 */
@Controller("/api/messages")
public class MessageApiController {

    private final MessageService messageService;

    public MessageApiController(MessageService messageService) {
        this.messageService = messageService;
    }

    @Post(consumes = MediaType.MULTIPART_FORM_DATA, produces = MediaType.APPLICATION_JSON)
    public HttpResponse<CreateMessageResponse> create(
            @Part CompletedFileUpload blob,
            @Part String pin,
            @Part String ttl
    ) {
        CreateCommand command = new CreateCommand(readBytes(blob), pin, parseTtl(ttl));
        CreateResult result = messageService.create(command);
        return HttpResponse.created(new CreateMessageResponse(result.id(), result.expiresAt()));
    }

    @Get(uri = "/{id}/retrieve", produces = MediaType.APPLICATION_OCTET_STREAM)
    public HttpResponse<byte[]> retrieve(
            @PathVariable String id,
            @Header("X-Message-Pin") String pin
    ) {
        byte[] blob = messageService.retrieve(id, pin);
        return HttpResponse.ok(blob).contentType(MediaType.APPLICATION_OCTET_STREAM_TYPE);
    }

    private static long parseTtl(String ttl) {
        try {
            return Long.parseLong(ttl);
        } catch (NumberFormatException exception) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid TTL value");
        }
    }

    private static byte[] readBytes(CompletedFileUpload blob) {
        try {
            return blob.getBytes();
        } catch (IOException exception) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid request payload");
        }
    }

}
