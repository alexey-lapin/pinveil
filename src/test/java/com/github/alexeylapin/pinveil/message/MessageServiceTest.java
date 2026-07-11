package com.github.alexeylapin.pinveil.message;

import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import com.github.alexeylapin.pinveil.passphrase.DicewareService;
import com.github.alexeylapin.pinveil.security.PinVerifier;
import org.junit.jupiter.api.Test;

import java.time.Duration;
import java.time.Instant;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class MessageServiceTest {

    @Test
    void retrievesAndDeletesStoredMessage() {
        MessageService service = createService();

        CreateResult result = service.create(command("123456", 60));
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
        CreateResult result = service.create(command("123456", 60));

        for (int attempt = 0; attempt < 3; attempt++) {
            assertThrows(MessageException.class, () -> service.retrieve(result.id(), "000000"));
        }

        MessageException exception = assertThrows(MessageException.class, () -> service.retrieve(result.id(), "123456"));
        assertEquals(MessageError.NOT_FOUND, exception.error());
    }

    @Test
    void removesExpiredMessagesOnAccess() throws InterruptedException {
        MessageService service = createService();
        CreateResult result = service.create(command("654321", 1));

        Thread.sleep(1200);

        MessageException exception = assertThrows(MessageException.class, () -> service.retrieve(result.id(), "654321"));
        assertEquals(MessageError.NOT_FOUND, exception.error());
    }

    @Test
    void rejectsInvalidPinOnCreate() {
        MessageService service = createService();
        MessageException exception = assertThrows(MessageException.class, () -> service.create(command("12ab56", 60)));
        assertEquals(MessageError.INVALID_REQUEST, exception.error());
    }

    @Test
    void rejectsEmptyPayload() {
        MessageService service = createService();
        CreateCommand command = new CreateCommand(new byte[0], "123456", 60);
        MessageException exception = assertThrows(MessageException.class, () -> service.create(command));
        assertEquals(MessageError.PAYLOAD_TOO_LARGE, exception.error());
    }

    @Test
    void rejectsTtlOutsideAllowedRange() {
        MessageService service = createService();
        MessageException exception = assertThrows(MessageException.class, () -> service.create(command("123456", 999_999)));
        assertEquals(MessageError.INVALID_REQUEST, exception.error());
    }

    private CreateCommand command(String pin, long ttlSeconds) {
        return new CreateCommand("opaque-blob-content".getBytes(), pin, ttlSeconds);
    }

    private MessageService createService() {
        MessagePolicyConfig policy = new MessagePolicyConfig();
        policy.setMinTtl(Duration.ofSeconds(1));
        policy.setMaxTtl(Duration.ofHours(24));

        PinSecurityConfig pinSecurity = new PinSecurityConfig();
        pinSecurity.setPepper("test-pepper");

        return new DefaultMessageService(new FakeMessageStore(), policy, pinSecurity, new PinVerifier(pinSecurity), new DicewareService());
    }

    /** Minimal in-memory store so the service can be tested in isolation. */
    private static final class FakeMessageStore implements MessageStore {

        private final Map<String, StoredMessage> messages = new HashMap<>();

        @Override
        public void save(StoredMessage message) {
            messages.put(message.id(), message);
        }

        @Override
        public Optional<StoredMessage> find(String id) {
            return Optional.ofNullable(messages.get(id));
        }

        @Override
        public boolean remove(String id) {
            return messages.remove(id) != null;
        }

        @Override
        public boolean contains(String id) {
            return messages.containsKey(id);
        }

        @Override
        public int count() {
            return messages.size();
        }

        @Override
        public int removeExpired(Instant now) {
            int removed = 0;
            Iterator<StoredMessage> iterator = messages.values().iterator();
            while (iterator.hasNext()) {
                if (iterator.next().isExpiredAt(now)) {
                    iterator.remove();
                    removed++;
                }
            }
            return removed;
        }

    }

}
