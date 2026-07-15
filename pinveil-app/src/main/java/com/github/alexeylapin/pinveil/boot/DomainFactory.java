package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.InMemoryStoreConfig;
import com.github.alexeylapin.pinveil.config.MessageIdConfig;
import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import com.github.alexeylapin.pinveil.config.RateLimitConfig;
import com.github.alexeylapin.pinveil.diceware.DefaultDicewareService;
import com.github.alexeylapin.pinveil.diceware.DicewareIdGenerator;
import com.github.alexeylapin.pinveil.diceware.DicewareService;
import com.github.alexeylapin.pinveil.message.DefaultMessageService;
import com.github.alexeylapin.pinveil.message.InMemoryMessageStore;
import com.github.alexeylapin.pinveil.message.MessageService;
import com.github.alexeylapin.pinveil.message.MessageStore;
import com.github.alexeylapin.pinveil.ratelimit.InMemoryRateLimiter;
import com.github.alexeylapin.pinveil.ratelimit.RateLimiter;
import com.github.alexeylapin.pinveil.security.Argon2PinVerifier;
import com.github.alexeylapin.pinveil.security.PinVerifier;
import io.micronaut.context.annotation.Factory;
import jakarta.inject.Singleton;

import java.util.function.Supplier;

/**
 * Wires the framework-free core into Micronaut-managed singletons. This is the one
 * place that knows which concrete implementations back each abstraction.
 */
@Factory
public class DomainFactory {

    @Singleton
    MessageStore messageStore(InMemoryStoreConfig config) {
        return new InMemoryMessageStore(config);
    }

    @Singleton
    DicewareService dicewareService() {
        return new DefaultDicewareService();
    }

    @Singleton
    Supplier<String> messageIdGenerator(DicewareService diceware, MessageIdConfig config) {
        return new DicewareIdGenerator(diceware, config);
    }

    @Singleton
    PinVerifier pinVerifier(PinSecurityConfig config) {
        return new Argon2PinVerifier(config);
    }

    @Singleton
    RateLimiter rateLimiter(RateLimitConfig config) {
        return new InMemoryRateLimiter(config);
    }

    @Singleton
    MessageService messageService(MessageStore store,
                                  MessagePolicyConfig policy,
                                  PinSecurityConfig pinSecurity,
                                  PinVerifier pinVerifier,
                                  Supplier<String> messageIdGenerator) {
        return new DefaultMessageService(store, policy, pinSecurity, pinVerifier, messageIdGenerator);
    }

}
