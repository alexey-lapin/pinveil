package com.github.alexeylapin.pinveil.message;

import java.time.Instant;
import java.util.Optional;

/**
 * Storage seam for {@link StoredMessage}s.
 *
 * <p>The contract is deliberately limited to storing, looking up, and removing
 * messages. Implementations choose their own persistence medium (heap, disk,
 * relational database, ...) along with their own capacity and durability
 * characteristics; none of that leaks through this interface. This lets the
 * domain layer work against a single abstraction while the concrete backend is
 * swapped underneath.
 *
 * <p>Implementations must be safe for concurrent use.
 */
public interface MessageStore {

    /**
     * Stores the given message, replacing any existing message with the same id.
     *
     * @param message the message to store.
     * @throws MessageStoreException if the message cannot be stored, for example
     *                               because the backend is full or unavailable.
     */
    void save(StoredMessage message);

    /**
     * Looks up a message by its id.
     *
     * @param id the message id.
     * @return the stored message, or an empty {@link Optional} if none is stored under {@code id}.
     */
    Optional<StoredMessage> find(String id);

    /**
     * Removes the message with the given id, if present.
     *
     * @param id the message id.
     * @return {@code true} if a message was removed, {@code false} if none existed.
     */
    boolean remove(String id);

    /**
     * Reports whether a message is currently stored under the given id.
     *
     * @param id the message id.
     * @return {@code true} if a message is stored under {@code id}.
     */
    boolean contains(String id);

    /**
     * Returns the number of messages currently stored.
     *
     * @return the current message count.
     */
    int count();

    /**
     * Removes every message whose expiry is at or before {@code now}.
     *
     * @param now the reference instant to evaluate expiry against.
     * @return the number of messages removed.
     */
    int removeExpired(Instant now);

}
