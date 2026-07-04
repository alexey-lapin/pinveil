package com.github.alexeylapin.pinveil.message;

import org.junit.jupiter.api.Test;

import java.time.Instant;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class InMemoryMessageStoreTest {

    private final InMemoryMessageStore store = new InMemoryMessageStore();

    @Test
    void savesAndFindsById() {
        store.save(message("alpha", 10, Instant.now().plusSeconds(60)));

        assertTrue(store.contains("alpha"));
        assertTrue(store.find("alpha").isPresent());
        assertEquals(1, store.count());
    }

    @Test
    void tracksStoredBytesAcrossSaveAndRemove() {
        store.save(message("alpha", 10, Instant.now().plusSeconds(60)));
        store.save(message("beta", 25, Instant.now().plusSeconds(60)));
        assertEquals(35, store.storedBytes());

        assertTrue(store.remove("alpha"));
        assertEquals(25, store.storedBytes());
        assertFalse(store.remove("alpha"));
    }

    @Test
    void removeExpiredDropsOnlyExpiredAndFixesByteCount() {
        Instant now = Instant.now();
        store.save(message("expired", 10, now.minusSeconds(1)));
        store.save(message("live", 20, now.plusSeconds(60)));

        int removed = store.removeExpired(now);

        assertEquals(1, removed);
        assertFalse(store.contains("expired"));
        assertTrue(store.contains("live"));
        assertEquals(20, store.storedBytes());
    }

    private static StoredMessage message(String id, int size, Instant expiresAt) {
        return new StoredMessage(id, new byte[size], "verifier", "salt", expiresAt);
    }

}
