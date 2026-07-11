package com.github.alexeylapin.pinveil.message;

import com.github.alexeylapin.pinveil.config.InMemoryStoreConfig;
import io.micronaut.scheduling.annotation.Scheduled;
import jakarta.inject.Singleton;

import java.time.Instant;
import java.util.Iterator;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;

/**
 * In-memory {@link MessageStore} backed by a map. It guards its own heap usage by
 * capping message count and total bytes; when a save would exceed a cap it first
 * sweeps expired messages to reclaim space, and only then rejects. All access is
 * synchronized so the byte total and map stay consistent, including against the
 * scheduled sweep running on another thread.
 */
@Singleton
public class InMemoryMessageStore implements MessageStore {

    private final InMemoryStoreConfig config;
    private final Map<String, StoredMessage> messages = new ConcurrentHashMap<>();
    private long storedBytes;

    public InMemoryMessageStore(InMemoryStoreConfig config) {
        this.config = config;
    }

    @Override
    public synchronized void save(StoredMessage message) {
        if (!admits(message)) {
            removeExpired(Instant.now());
            if (!admits(message)) {
                throw new MessageStoreException("In-memory store capacity reached");
            }
        }
        StoredMessage previous = messages.put(message.id(), message);
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

    /** Whether the store can hold {@code message} without exceeding its caps. */
    private boolean admits(StoredMessage message) {
        StoredMessage existing = messages.get(message.id());
        int projectedCount = messages.size() + (existing == null ? 1 : 0);
        long projectedBytes = storedBytes + message.sizeInBytes() - (existing == null ? 0 : existing.sizeInBytes());
        return projectedCount <= config.getMaxMessages() && projectedBytes <= config.getMaxBytes();
    }

    @Scheduled(fixedDelay = "1m")
    void sweepExpired() {
        removeExpired(Instant.now());
    }

}
