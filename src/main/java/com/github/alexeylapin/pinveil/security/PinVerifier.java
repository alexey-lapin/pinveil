package com.github.alexeylapin.pinveil.security;

/**
 * Hashes and verifies PINs. Implementations choose the hashing scheme and cost.
 */
public interface PinVerifier {

    /**
     * Hashes a PIN into an opaque verifier that can later confirm the same PIN.
     *
     * @param pin     the PIN to protect.
     * @param pinSalt a per-message salt.
     * @return an opaque verifier string.
     */
    String hash(String pin, String pinSalt);

    /**
     * Checks a PIN against a previously produced verifier.
     *
     * @param pin      the PIN to check.
     * @param pinSalt  the salt the verifier was produced with.
     * @param verifier the stored verifier.
     * @return {@code true} if the PIN and salt reproduce the verifier.
     */
    boolean verify(String pin, String pinSalt, String verifier);

}
