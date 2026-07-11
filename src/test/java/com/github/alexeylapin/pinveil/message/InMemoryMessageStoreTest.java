package com.github.alexeylapin.pinveil.message;

import com.github.alexeylapin.pinveil.config.InMemoryStoreConfig;
import org.junit.jupiter.api.Test;

import java.time.Instant;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class InMemoryMessageStoreTest {

    @Test
    void savesAndFindsById() {
        InMemoryMessageStore store = store(10, 1024);
        store.save(message("alpha", 10, Instant.now().plusSeconds(60)));

        assertTrue(store.contains("alpha"));
        assertTrue(store.find("alpha").isPresent());
        assertEquals(1, store.count());
    }

    @Test
    void rejectsSaveWhenMessageCountCapReached() {
        InMemoryMessageStore store = store(2, 1024);
        store.save(message("a", 1, Instant.now().plusSeconds(60)));
        store.save(message("b", 1, Instant.now().plusSeconds(60)));

        assertThrows(MessageStoreException.class,
                () -> store.save(message("c", 1, Instant.now().plusSeconds(60))));
        assertEquals(2, store.count());
    }

    @Test
    void rejectsSaveWhenByteCapReached() {
        InMemoryMessageStore store = store(10, 30);
        store.save(message("a", 20, Instant.now().plusSeconds(60)));

        assertThrows(MessageStoreException.class,
                () -> store.save(message("b", 20, Instant.now().plusSeconds(60))));
    }

    @Test
    void sweepsExpiredToReclaimSpaceBeforeRejecting() {
        InMemoryMessageStore store = store(10, 30);
        store.save(message("stale", 20, Instant.now().minusSeconds(1)));

        // Would exceed the byte cap alongside the stale message, but sweeping it frees room.
        store.save(message("fresh", 20, Instant.now().plusSeconds(60)));

        assertFalse(store.contains("stale"));
        assertTrue(store.contains("fresh"));
        assertEquals(1, store.count());
    }

    @Test
    void removeExpiredDropsOnlyExpired() {
        InMemoryMessageStore store = store(10, 1024);
        Instant now = Instant.now();
        store.save(message("expired", 10, now.minusSeconds(1)));
        store.save(message("live", 20, now.plusSeconds(60)));

        int removed = store.removeExpired(now);

        assertEquals(1, removed);
        assertFalse(store.contains("expired"));
        assertTrue(store.contains("live"));
    }

    private static InMemoryMessageStore store(int maxMessages, long maxBytes) {
        InMemoryStoreConfig config = new InMemoryStoreConfig();
        config.setMaxMessages(maxMessages);
        config.setMaxBytes(maxBytes);
        return new InMemoryMessageStore(config);
    }

    private static StoredMessage message(String id, int size, Instant expiresAt) {
        return new StoredMessage(id, new byte[size], "verifier", "salt", expiresAt);
    }

}
