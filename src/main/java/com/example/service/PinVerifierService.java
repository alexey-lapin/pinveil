package com.example.service;

import com.example.config.MessageConfiguration;
import de.mkammerer.argon2.Argon2;
import de.mkammerer.argon2.Argon2Factory;
import jakarta.inject.Singleton;

@Singleton
public class PinVerifierService {

    private static final int ITERATIONS = 3;
    private static final int MEMORY_KIB = 19_456;
    private static final int PARALLELISM = 1;

    private final MessageConfiguration configuration;

    public PinVerifierService(MessageConfiguration configuration) {
        this.configuration = configuration;
    }

    public String hash(String pin, String pinSalt) {
        Argon2 argon2 = Argon2Factory.create(Argon2Factory.Argon2Types.ARGON2id);
        char[] input = combinedInput(pin, pinSalt);
        try {
            return argon2.hash(ITERATIONS, MEMORY_KIB, PARALLELISM, input);
        } finally {
            argon2.wipeArray(input);
        }
    }

    public boolean verify(String pin, String pinSalt, String verifier) {
        Argon2 argon2 = Argon2Factory.create(Argon2Factory.Argon2Types.ARGON2id);
        char[] input = combinedInput(pin, pinSalt);
        try {
            return argon2.verify(verifier, input);
        } finally {
            argon2.wipeArray(input);
        }
    }

    private char[] combinedInput(String pin, String pinSalt) {
        return (pin + ":" + pinSalt + ":" + configuration.getPinPepper()).toCharArray();
    }

}
