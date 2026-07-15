package com.github.alexeylapin.pinveil.ratelimit;

/**
 * Rate-limit seam. Implementations decide whether a caller may proceed within a
 * named bucket, keyed by a caller-supplied subject.
 */
public interface RateLimiter {

    /**
     * @param bucket the limit bucket to apply.
     * @param key    the subject to count requests against.
     * @return {@code true} if the request is allowed and counted, {@code false} if the limit is exceeded.
     */
    boolean tryAcquire(String bucket, String key);

}
