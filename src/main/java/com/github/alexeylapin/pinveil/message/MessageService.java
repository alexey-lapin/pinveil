package com.github.alexeylapin.pinveil.message;

/**
 * Application service for the message lifecycle: create an encrypted message and
 * retrieve it once with the correct PIN. Implementations own validation and the
 * coordination of storage and PIN verification.
 */
public interface MessageService {

    /**
     * Creates and stores a message from the given command.
     *
     * @param command the blob, PIN, and lifetime to store.
     * @return the id and expiry of the stored message.
     * @throws MessageException if the command is invalid or the store cannot accept it.
     */
    CreateResult create(CreateCommand command);

    /**
     * Retrieves and consumes the message with the given id. A successful call
     * removes the message; failed PIN attempts are recorded and eventually burn it down.
     *
     * @param id  the message id.
     * @param pin the six-digit PIN.
     * @return the stored ciphertext blob.
     * @throws MessageException if the PIN is malformed, the message is missing or expired,
     *                          or the PIN does not match.
     */
    byte[] retrieve(String id, String pin);

    /**
     * @return the number of messages currently stored.
     */
    int storedMessageCount();

}
