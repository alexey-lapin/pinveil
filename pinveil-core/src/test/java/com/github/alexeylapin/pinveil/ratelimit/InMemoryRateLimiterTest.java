package com.github.alexeylapin.pinveil.ratelimit;

import com.github.alexeylapin.pinveil.config.RateLimitConfig;
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

    private static InMemoryRateLimiter limiter(int createLimit, int retrieveLimit, Duration window) {
        RateLimitConfig config = new RateLimitConfig();
        config.setCreateRequestsPerWindow(createLimit);
        config.setRetrieveRequestsPerWindow(retrieveLimit);
        config.setWindow(window);
        return new InMemoryRateLimiter(config);
    }

}
