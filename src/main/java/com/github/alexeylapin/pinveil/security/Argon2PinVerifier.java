package com.github.alexeylapin.pinveil.security;

import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import de.mkammerer.argon2.Argon2;
import de.mkammerer.argon2.Argon2Factory;
import jakarta.inject.Singleton;

/**
 * {@link PinVerifier} backed by Argon2id, with each PIN salted per message and
 * peppered with a server secret. Cost parameters come from {@link PinSecurityConfig}.
 */
@Singleton
public class Argon2PinVerifier implements PinVerifier {

    private final PinSecurityConfig config;
    private final Argon2 argon2 = Argon2Factory.create(Argon2Factory.Argon2Types.ARGON2id);

    public Argon2PinVerifier(PinSecurityConfig config) {
        this.config = config;
    }

    @Override
    public String hash(String pin, String pinSalt) {
        char[] input = combinedInput(pin, pinSalt);
        try {
            return argon2.hash(config.getArgon2Iterations(), config.getArgon2MemoryKib(), config.getArgon2Parallelism(), input);
        } finally {
            argon2.wipeArray(input);
        }
    }

    @Override
    public boolean verify(String pin, String pinSalt, String verifier) {
        char[] input = combinedInput(pin, pinSalt);
        try {
            return argon2.verify(verifier, input);
        } finally {
            argon2.wipeArray(input);
        }
    }

    private char[] combinedInput(String pin, String pinSalt) {
        return (pin + ":" + pinSalt + ":" + config.getPepper()).toCharArray();
    }

}
