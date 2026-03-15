package com.example.service;

import com.example.config.MessageConfiguration;
import org.junit.jupiter.api.Test;

import java.time.Duration;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class MessageServiceTest {
    @Test
    void retrievesAndDeletesStoredMessage() {
        MessageService service = createService();

        MessageService.CreateResult result = service.create(command("123456", 60));
        assertNotNull(result.id());
        assertTrue(result.id().matches("^[a-z]+-[a-z]+-[a-z]+-\\d{6}$"));

        assertEquals(1, service.storedMessageCount());
        byte[] blob = service.retrieve(result.id(), "123456");
        assertNotNull(blob);
        assertEquals(0, service.storedMessageCount());
    }

    @Test
    void deletesMessageAfterThreeFailedPins() {
        MessageService service = createService();
        MessageService.CreateResult result = service.create(command("123456", 60));

        assertThrows(MessageService.MessageException.class, () -> service.retrieve(result.id(), "000000"));
        assertThrows(MessageService.MessageException.class, () -> service.retrieve(result.id(), "000000"));
        assertThrows(MessageService.MessageException.class, () -> service.retrieve(result.id(), "000000"));

        MessageService.MessageException exception = assertThrows(
            MessageService.MessageException.class,
            () -> service.retrieve(result.id(), "123456")
        );

        assertEquals(MessageService.MessageError.NOT_FOUND, exception.error());
    }

    @Test
    void removesExpiredMessagesOnAccess() throws InterruptedException {
        MessageService service = createService();
        MessageService.CreateResult result = service.create(command("654321", 1));

        Thread.sleep(1200);

        MessageService.MessageException exception = assertThrows(
            MessageService.MessageException.class,
            () -> service.retrieve(result.id(), "654321")
        );

        assertEquals(MessageService.MessageError.NOT_FOUND, exception.error());
    }

    private MessageService.CreateCommand command(String pin, long ttlSeconds) {
        return new MessageService.CreateCommand(
            "opaque-blob-content".getBytes(),
            pin,
            ttlSeconds
        );
    }

    private MessageService createService() {
        MessageConfiguration configuration = configuration();
        return new MessageService(configuration, new PinVerifierService(configuration), new DicewareService());
    }

    private MessageConfiguration configuration() {
        MessageConfiguration configuration = new MessageConfiguration();
        configuration.setMinTtl(Duration.ofSeconds(1));
        configuration.setMaxTtl(Duration.ofHours(24));
        configuration.setDefaultTtl(Duration.ofMinutes(15));
        configuration.setPbkdf2Iterations(600000);
        configuration.setPinPepper("test-pepper");
        return configuration;
    }
}
