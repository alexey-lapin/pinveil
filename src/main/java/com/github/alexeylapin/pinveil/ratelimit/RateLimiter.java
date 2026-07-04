package com.github.alexeylapin.pinveil.ratelimit;

/**
 * Rate-limit seam. Implementations decide whether a caller may proceed within a
 * named bucket (e.g. "create", "retrieve") keyed by some subject such as client IP.
 */
public interface RateLimiter {

    /**
     * @return {@code true} if the request is allowed and counted, {@code false} if the limit is exceeded.
     */
    boolean tryAcquire(String bucket, String key);

}
