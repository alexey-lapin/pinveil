package com.github.alexeylapin.pinveil.ratelimit;

import com.github.alexeylapin.pinveil.config.RateLimitConfig;
import jakarta.inject.Singleton;

import java.time.Instant;
import java.util.Iterator;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * In-memory fixed-window rate limiter. Each (bucket, key) pair gets a counter that
 * resets once its window elapses; stale counters are purged opportunistically.
 */
@Singleton
public class InMemoryRateLimiter implements RateLimiter {

    private final RateLimitConfig config;
    private final Map<String, Window> windows = new ConcurrentHashMap<>();

    public InMemoryRateLimiter(RateLimitConfig config) {
        this.config = config;
    }

    @Override
    public synchronized boolean tryAcquire(String bucket, String key) {
        int limit = config.limitFor(bucket);
        Instant now = Instant.now();
        purgeExpired(now);

        Window window = windows.get(bucket + ":" + key);
        if (window == null || window.endsAt().isBefore(now)) {
            window = new Window(now.plus(config.getWindow()));
            windows.put(bucket + ":" + key, window);
        }
        if (window.count() >= limit) {
            return false;
        }
        window.increment();
        return true;
    }

    private void purgeExpired(Instant now) {
        Iterator<Map.Entry<String, Window>> iterator = windows.entrySet().iterator();
        while (iterator.hasNext()) {
            if (iterator.next().getValue().endsAt().isBefore(now)) {
                iterator.remove();
            }
        }
    }

    private static final class Window {

        private final Instant endsAt;
        private int count;

        private Window(Instant endsAt) {
            this.endsAt = endsAt;
        }

        private Instant endsAt() {
            return endsAt;
        }

        private int count() {
            return count;
        }

        private void increment() {
            count++;
        }

    }

}
