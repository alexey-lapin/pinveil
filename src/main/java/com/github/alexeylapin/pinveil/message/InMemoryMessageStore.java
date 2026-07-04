package com.github.alexeylapin.pinveil.message;

import io.micronaut.scheduling.annotation.Scheduled;
import jakarta.inject.Singleton;

import java.time.Instant;
import java.util.Iterator;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * In-memory {@link MessageStore} backed by a map, tracking total stored bytes so
 * the accounting can never drift from the contents. All access is synchronized so
 * the byte total and map stay consistent.
 */
@Singleton
public class InMemoryMessageStore implements MessageStore {

    private final Map<String, StoredMessage> messages = new ConcurrentHashMap<>();
    private long storedBytes;

    @Override
    public synchronized void save(StoredMessage message) {
        StoredMessage previous = messages.put(message.getId(), message);
        if (previous != null) {
            storedBytes -= previous.sizeInBytes();
        }
        storedBytes += message.sizeInBytes();
    }

    @Override
    public synchronized Optional<StoredMessage> find(String id) {
        return Optional.ofNullable(messages.get(id));
    }

    @Override
    public synchronized boolean remove(String id) {
        StoredMessage removed = messages.remove(id);
        if (removed == null) {
            return false;
        }
        storedBytes -= removed.sizeInBytes();
        return true;
    }

    @Override
    public synchronized boolean contains(String id) {
        return messages.containsKey(id);
    }

    @Override
    public synchronized int count() {
        return messages.size();
    }

    @Override
    public synchronized long storedBytes() {
        return storedBytes;
    }

    @Override
    public synchronized int removeExpired(Instant now) {
        int removed = 0;
        Iterator<Map.Entry<String, StoredMessage>> iterator = messages.entrySet().iterator();
        while (iterator.hasNext()) {
            StoredMessage message = iterator.next().getValue();
            if (message.isExpiredAt(now)) {
                storedBytes -= message.sizeInBytes();
                iterator.remove();
                removed++;
            }
        }
        return removed;
    }

    @Scheduled(fixedDelay = "1m")
    void sweepExpired() {
        removeExpired(Instant.now());
    }

}
