package com.github.alexeylapin.pinveil.security;

import de.mkammerer.argon2.Argon2;
import de.mkammerer.argon2.Argon2Factory;

/**
 * {@link PinVerifier} backed by Argon2id, with each PIN salted per message and
 * peppered with a server secret. Cost parameters come from {@link Argon2Params}.
 */
public class Argon2PinVerifier implements PinVerifier {

    private final Argon2Params params;
    private final Argon2 argon2 = Argon2Factory.create(Argon2Factory.Argon2Types.ARGON2id);

    public Argon2PinVerifier(Argon2Params params) {
        this.params = params;
    }

    @Override
    public String hash(String pin, String pinSalt) {
        char[] input = combinedInput(pin, pinSalt);
        try {
            return argon2.hash(params.iterations(), params.memoryKib(), params.parallelism(), input);
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
        return (pin + ":" + pinSalt + ":" + params.pepper()).toCharArray();
    }

}
