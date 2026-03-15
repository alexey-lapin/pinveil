package com.example.service;

import com.example.config.MessageConfiguration;
import com.example.model.StoredMessage;
import io.micronaut.scheduling.annotation.Scheduled;
import jakarta.inject.Singleton;

import java.security.SecureRandom;
import java.time.Duration;
import java.time.Instant;
import java.util.Base64;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.regex.Pattern;

@Singleton
public class MessageService {
    private static final Pattern PIN_PATTERN = Pattern.compile("^\\d{6}$");
    private static final int MAX_ID_ATTEMPTS = 10;

    private final Map<String, StoredMessage> messages = new HashMap<>();
    private final MessageConfiguration configuration;
    private final PinVerifierService pinVerifierService;
    private final DicewareService dicewareService;
    private final SecureRandom secureRandom = new SecureRandom();
    private long storedBytes;

    public MessageService(MessageConfiguration configuration, PinVerifierService pinVerifierService, DicewareService dicewareService) {
        this.configuration = configuration;
        this.pinVerifierService = pinVerifierService;
        this.dicewareService = dicewareService;
        this.storedBytes = 0;
    }

    public synchronized CreateResult create(CreateCommand command) {
        cleanupExpiredInternal(Instant.now());
        validateCreateCommand(command);

        if (messages.size() >= configuration.getMaxStoredMessages()) {
            throw new MessageException(MessageError.CAPACITY_REACHED, "Server storage capacity reached");
        }

        long incomingBytes = command.blob().length;
        if (storedBytes + incomingBytes > configuration.getMaxStoredBytes()) {
            throw new MessageException(MessageError.CAPACITY_REACHED, "Server storage capacity reached");
        }

        String id = generateUniqueId();
        Instant now = Instant.now();
        Instant expiresAt = now.plusSeconds(command.ttlSeconds());
        String pinSalt = randomBase64(16);
        String pinVerifier = pinVerifierService.hash(command.pin(), pinSalt);

        StoredMessage storedMessage = new StoredMessage(
            id,
            command.blob().clone(),
            pinVerifier,
            pinSalt,
            expiresAt,
            now
        );

        messages.put(id, storedMessage);
        storedBytes += storedMessage.sizeInBytes();
        return new CreateResult(id, expiresAt);
    }

    public synchronized byte[] retrieve(String id, String pin) {
        if (!isValidPin(pin)) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid PIN format");
        }

        Instant now = Instant.now();
        cleanupExpiredInternal(now);

        StoredMessage storedMessage = messages.get(id);
        if (storedMessage == null) {
            throw new MessageException(MessageError.NOT_FOUND, "Message not found");
        }
        if (storedMessage.getExpiresAt().isBefore(now)) {
            removeInternal(id);
            throw new MessageException(MessageError.NOT_FOUND, "Message not found");
        }

        if (!pinVerifierService.verify(pin, storedMessage.getPinSalt(), storedMessage.getPinVerifier())) {
            storedMessage.incrementFailedPinAttempts();
            if (storedMessage.getFailedPinAttempts() >= configuration.getMaxFailedPinAttempts()) {
                removeInternal(id);
            }
            throw new MessageException(MessageError.FORBIDDEN, "Unable to retrieve message");
        }

        removeInternal(id);
        return storedMessage.getBlob();
    }

    public synchronized int storedMessageCount() {
        cleanupExpiredInternal(Instant.now());
        return messages.size();
    }

    public static boolean isValidPin(String pin) {
        return pin != null && PIN_PATTERN.matcher(pin).matches();
    }

    @Scheduled(fixedDelay = "1m")
    void cleanupExpired() {
        synchronized (this) {
            cleanupExpiredInternal(Instant.now());
        }
    }

    private String generateUniqueId() {
        for (int attempt = 0; attempt < MAX_ID_ATTEMPTS; attempt++) {
            String id = dicewareService.generateMessageId();
            if (!messages.containsKey(id)) {
                return id;
            }
        }
        throw new MessageException(MessageError.CAPACITY_REACHED, "Unable to generate unique id");
    }

    private void validateCreateCommand(CreateCommand command) {
        if (!isValidPin(command.pin())) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid PIN format");
        }
        if (command.blob().length == 0 || command.blob().length > configuration.getMaxPayloadBytes()) {
            throw new MessageException(MessageError.PAYLOAD_TOO_LARGE, "Blob exceeds maximum size");
        }
        validateTtl(command.ttlSeconds());
    }

    private void validateTtl(long ttlSeconds) {
        Duration ttl = Duration.ofSeconds(ttlSeconds);
        if (ttl.compareTo(configuration.getMinTtl()) < 0 || ttl.compareTo(configuration.getMaxTtl()) > 0) {
            throw new MessageException(MessageError.INVALID_REQUEST, "TTL is outside the allowed range");
        }
    }

    private void cleanupExpiredInternal(Instant now) {
        Iterator<Map.Entry<String, StoredMessage>> iterator = messages.entrySet().iterator();
        while (iterator.hasNext()) {
            Map.Entry<String, StoredMessage> entry = iterator.next();
            if (!entry.getValue().getExpiresAt().isAfter(now)) {
                storedBytes -= entry.getValue().sizeInBytes();
                iterator.remove();
            }
        }
    }

    private void removeInternal(String id) {
        StoredMessage removed = messages.remove(id);
        if (removed != null) {
            storedBytes -= removed.sizeInBytes();
        }
    }

    private String randomBase64(int byteCount) {
        byte[] bytes = new byte[byteCount];
        secureRandom.nextBytes(bytes);
        return Base64.getEncoder().encodeToString(bytes);
    }

    public record CreateCommand(
        byte[] blob,
        String pin,
        long ttlSeconds
    ) {
    }

    public record CreateResult(String id, Instant expiresAt) {
    }

    public enum MessageError {
        INVALID_REQUEST,
        PAYLOAD_TOO_LARGE,
        CAPACITY_REACHED,
        FORBIDDEN,
        NOT_FOUND
    }

    public static final class MessageException extends RuntimeException {
        private final MessageError error;

        public MessageException(MessageError error, String message) {
            super(message);
            this.error = error;
        }

        public MessageError error() {
            return error;
        }
    }
}
