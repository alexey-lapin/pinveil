package com.github.alexeylapin.pinveil.ratelimit;

import org.junit.jupiter.api.Test;

import java.time.Duration;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

class InMemoryRateLimiterTest {

    @Test
    void allowsUpToLimitThenBlocks() {
        InMemoryRateLimiter limiter = limiter(2, 3, Duration.ofMinutes(1));

        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
        assertFalse(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
    }

    @Test
    void countsBucketsAndKeysIndependently() {
        InMemoryRateLimiter limiter = limiter(1, 1, Duration.ofMinutes(1));

        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
        assertFalse(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
        // Different bucket, same key.
        assertTrue(limiter.tryAcquire(RateLimitConfig.RETRIEVE_BUCKET, "ip"));
        // Same bucket, different key.
        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "other-ip"));
    }

    @Test
    void resetsAfterWindowElapses() throws InterruptedException {
        InMemoryRateLimiter limiter = limiter(1, 1, Duration.ofMillis(200));

        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
        assertFalse(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));

        Thread.sleep(250);

        assertTrue(limiter.tryAcquire(RateLimitConfig.CREATE_BUCKET, "ip"));
    }

    /** Stands in for the Micronaut-generated binding of {@link RateLimitConfig}. */
    private static InMemoryRateLimiter limiter(int createLimit, int retrieveLimit, Duration window) {
        return new InMemoryRateLimiter(new RateLimitConfig() {

            @Override
            public Duration getWindow() {
                return window;
            }

            @Override
            public int getCreateRequestsPerWindow() {
                return createLimit;
            }

            @Override
            public int getRetrieveRequestsPerWindow() {
                return retrieveLimit;
            }

        });
    }

}
