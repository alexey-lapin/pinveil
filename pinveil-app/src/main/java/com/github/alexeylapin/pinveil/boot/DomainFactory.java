package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.diceware.DefaultDicewareService;
import com.github.alexeylapin.pinveil.diceware.DicewareIdGenerator;
import com.github.alexeylapin.pinveil.diceware.DicewareService;
import com.github.alexeylapin.pinveil.message.DefaultMessageService;
import com.github.alexeylapin.pinveil.message.InMemoryMessageStore;
import com.github.alexeylapin.pinveil.message.MessagePolicy;
import com.github.alexeylapin.pinveil.message.MessageService;
import com.github.alexeylapin.pinveil.message.MessageStore;
import com.github.alexeylapin.pinveil.ratelimit.InMemoryRateLimiter;
import com.github.alexeylapin.pinveil.ratelimit.RateLimitConfig;
import com.github.alexeylapin.pinveil.ratelimit.RateLimiter;
import com.github.alexeylapin.pinveil.security.Argon2Params;
import com.github.alexeylapin.pinveil.security.Argon2PinVerifier;
import com.github.alexeylapin.pinveil.security.PinVerifier;
import io.micronaut.context.annotation.Factory;
import jakarta.inject.Singleton;

import java.util.function.Supplier;

/**
 * Wires the framework-free core into Micronaut-managed singletons. This is the one
 * place that knows which concrete implementations back each abstraction, and the one
 * place that turns bound properties into the core's own value types.
 */
@Factory
public class DomainFactory {

    @Singleton
    MessageStore messageStore(InMemoryStoreProperties properties) {
        return new InMemoryMessageStore(properties.getMaxMessages(), properties.getMaxBytes());
    }

    @Singleton
    DicewareService dicewareService() {
        return new DefaultDicewareService();
    }

    @Singleton
    Supplier<String> messageIdGenerator(DicewareService diceware, MessageIdProperties properties) {
        return new DicewareIdGenerator(diceware, properties.getWords(), properties.getDigits());
    }

    /** Assembled from two property sources: the TTL and payload limits, plus the PIN attempt budget. */
    @Singleton
    MessagePolicy messagePolicy(MessagePolicyProperties properties, PinSecurityProperties pinSecurity) {
        return new MessagePolicy(
                properties.getMinTtl(),
                properties.getMaxTtl(),
                properties.getMaxPayloadBytes(),
                pinSecurity.getMaxFailedAttempts()
        );
    }

    @Singleton
    PinVerifier pinVerifier(PinSecurityProperties properties) {
        return new Argon2PinVerifier(new Argon2Params(
                properties.getPepper(),
                properties.getArgon2Iterations(),
                properties.getArgon2MemoryKib(),
                properties.getArgon2Parallelism()
        ));
    }

    @Singleton
    RateLimiter rateLimiter(RateLimitConfig config) {
        return new InMemoryRateLimiter(config);
    }

    @Singleton
    MessageService messageService(MessageStore store,
                                  MessagePolicy policy,
                                  PinVerifier pinVerifier,
                                  Supplier<String> messageIdGenerator) {
        return new DefaultMessageService(store, policy, pinVerifier, messageIdGenerator);
    }

}
