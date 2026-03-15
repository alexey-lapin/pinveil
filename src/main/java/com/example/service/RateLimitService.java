package com.example.service;

import com.example.config.RateLimitConfiguration;
import jakarta.inject.Singleton;

import java.time.Instant;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

@Singleton
public class RateLimitService {
    private final RateLimitConfiguration configuration;
    private final Map<String, Counter> counters = new HashMap<>();

    public RateLimitService(RateLimitConfiguration configuration) {
        this.configuration = configuration;
    }

    public synchronized boolean allowCreate(String subject) {
        return allow("create:" + subject, configuration.getCreateRequestsPerWindow());
    }

    public synchronized boolean allowRetrieve(String subject) {
        return allow("retrieve:" + subject, configuration.getRetrieveRequestsPerWindow());
    }

    private boolean allow(String key, int limit) {
        Instant now = Instant.now();
        purgeExpired(now);
        Counter counter = counters.computeIfAbsent(key, ignored -> new Counter(now.plus(configuration.getWindow()), 0));
        if (counter.windowEndsAt().isBefore(now)) {
            counter = new Counter(now.plus(configuration.getWindow()), 0);
            counters.put(key, counter);
        }
        if (counter.count() >= limit) {
            return false;
        }
        counter.increment();
        return true;
    }

    private void purgeExpired(Instant now) {
        Iterator<Map.Entry<String, Counter>> iterator = counters.entrySet().iterator();
        while (iterator.hasNext()) {
            Map.Entry<String, Counter> entry = iterator.next();
            if (entry.getValue().windowEndsAt().isBefore(now)) {
                iterator.remove();
            }
        }
    }

    private static final class Counter {
        private final Instant windowEndsAt;
        private int count;

        private Counter(Instant windowEndsAt, int count) {
            this.windowEndsAt = windowEndsAt;
            this.count = count;
        }

        private Instant windowEndsAt() {
            return windowEndsAt;
        }

        private int count() {
            return count;
        }

        private void increment() {
            count++;
        }
    }
}
