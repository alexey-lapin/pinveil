package com.github.alexeylapin.pinveil.message;

import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import com.github.alexeylapin.pinveil.passphrase.DicewareService;
import com.github.alexeylapin.pinveil.security.PinVerifier;
import jakarta.inject.Singleton;

import java.security.SecureRandom;
import java.time.Duration;
import java.time.Instant;
import java.util.Base64;
import java.util.regex.Pattern;

/**
 * Orchestrates the message lifecycle: validates input, enforces capacity policy,
 * and coordinates the {@link MessageStore}, {@link PinVerifier} and
 * {@link DicewareService}. All storage and byte accounting lives behind the store.
 */
@Singleton
public class MessageService {

    private static final Pattern PIN_PATTERN = Pattern.compile("^\\d{6}$");
    private static final int MAX_ID_ATTEMPTS = 10;
    private static final int PIN_SALT_BYTES = 16;

    private final MessageStore store;
    private final MessagePolicyConfig policy;
    private final PinSecurityConfig pinSecurity;
    private final PinVerifier pinVerifier;
    private final DicewareService dicewareService;
    private final SecureRandom secureRandom = new SecureRandom();

    public MessageService(MessageStore store,
                          MessagePolicyConfig policy,
                          PinSecurityConfig pinSecurity,
                          PinVerifier pinVerifier,
                          DicewareService dicewareService) {
        this.store = store;
        this.policy = policy;
        this.pinSecurity = pinSecurity;
        this.pinVerifier = pinVerifier;
        this.dicewareService = dicewareService;
    }

    public synchronized CreateResult create(CreateCommand command) {
        Instant now = Instant.now();
        store.removeExpired(now);
        validateCreateCommand(command);
        enforceCapacity(command.blob().length);

        String id = generateUniqueId();
        Instant expiresAt = now.plusSeconds(command.ttlSeconds());
        String pinSalt = randomBase64(PIN_SALT_BYTES);
        String pinVerifierHash = pinVerifier.hash(command.pin(), pinSalt);

        store.save(new StoredMessage(id, command.blob().clone(), pinVerifierHash, pinSalt, expiresAt));
        return new CreateResult(id, expiresAt);
    }

    public synchronized byte[] retrieve(String id, String pin) {
        if (!isValidPin(pin)) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid PIN format");
        }

        Instant now = Instant.now();
        store.removeExpired(now);

        StoredMessage message = store.find(id)
                .orElseThrow(() -> new MessageException(MessageError.NOT_FOUND, "Message not found"));
        if (message.isExpiredAt(now)) {
            store.remove(id);
            throw new MessageException(MessageError.NOT_FOUND, "Message not found");
        }

        if (!pinVerifier.verify(pin, message.getPinSalt(), message.getPinVerifier())) {
            message.incrementFailedPinAttempts();
            if (message.getFailedPinAttempts() >= pinSecurity.getMaxFailedAttempts()) {
                store.remove(id);
            }
            throw new MessageException(MessageError.FORBIDDEN, "Unable to retrieve message");
        }

        store.remove(id);
        return message.getBlob();
    }

    public synchronized int storedMessageCount() {
        store.removeExpired(Instant.now());
        return store.count();
    }

    public static boolean isValidPin(String pin) {
        return pin != null && PIN_PATTERN.matcher(pin).matches();
    }

    private void validateCreateCommand(CreateCommand command) {
        if (!isValidPin(command.pin())) {
            throw new MessageException(MessageError.INVALID_REQUEST, "Invalid PIN format");
        }
        if (command.blob().length == 0 || command.blob().length > policy.getMaxPayloadBytes()) {
            throw new MessageException(MessageError.PAYLOAD_TOO_LARGE, "Blob exceeds maximum size");
        }
        Duration ttl = Duration.ofSeconds(command.ttlSeconds());
        if (ttl.compareTo(policy.getMinTtl()) < 0 || ttl.compareTo(policy.getMaxTtl()) > 0) {
            throw new MessageException(MessageError.INVALID_REQUEST, "TTL is outside the allowed range");
        }
    }

    private void enforceCapacity(long incomingBytes) {
        if (store.count() >= policy.getMaxStoredMessages()
                || store.storedBytes() + incomingBytes > policy.getMaxStoredBytes()) {
            throw new MessageException(MessageError.CAPACITY_REACHED, "Server storage capacity reached");
        }
    }

    private String generateUniqueId() {
        for (int attempt = 0; attempt < MAX_ID_ATTEMPTS; attempt++) {
            String id = dicewareService.generateMessageId();
            if (!store.contains(id)) {
                return id;
            }
        }
        throw new MessageException(MessageError.CAPACITY_REACHED, "Unable to generate unique id");
    }

    private String randomBase64(int byteCount) {
        byte[] bytes = new byte[byteCount];
        secureRandom.nextBytes(bytes);
        return Base64.getEncoder().encodeToString(bytes);
    }

}
