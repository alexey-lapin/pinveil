package com.github.alexeylapin.pinveil.message;

import java.time.Instant;
import java.util.Optional;

/**
 * Storage seam for {@link StoredMessage}s. Implementations own their own
 * persistence and byte accounting; callers enforce policy on top.
 */
public interface MessageStore {

    void save(StoredMessage message);

    Optional<StoredMessage> find(String id);

    boolean remove(String id);

    boolean contains(String id);

    int count();

    long storedBytes();

    /**
     * Removes every message that has expired as of {@code now}.
     *
     * @return the number of messages removed.
     */
    int removeExpired(Instant now);

}
