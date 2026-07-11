package com.github.alexeylapin.pinveil.ratelimit;

import com.github.alexeylapin.pinveil.config.RateLimitConfig;
import com.github.alexeylapin.pinveil.web.dto.ErrorResponse;
import io.micronaut.core.async.publisher.Publishers;
import io.micronaut.http.HttpMethod;
import io.micronaut.http.HttpRequest;
import io.micronaut.http.HttpResponse;
import io.micronaut.http.HttpStatus;
import io.micronaut.http.MutableHttpResponse;
import io.micronaut.http.annotation.Filter;
import io.micronaut.http.filter.HttpServerFilter;
import io.micronaut.http.filter.ServerFilterChain;
import io.micronaut.http.server.util.HttpClientAddressResolver;
import org.reactivestreams.Publisher;

/**
 * Enforces rate limits on the message API before requests reach the controller.
 * Resolves the real client IP (honouring proxy headers) so limits key on the
 * caller rather than the upstream proxy.
 */
@Filter("/api/messages/**")
public class RateLimitFilter implements HttpServerFilter {

    private final RateLimiter rateLimiter;
    private final HttpClientAddressResolver addressResolver;

    public RateLimitFilter(RateLimiter rateLimiter, HttpClientAddressResolver addressResolver) {
        this.rateLimiter = rateLimiter;
        this.addressResolver = addressResolver;
    }

    @Override
    public Publisher<MutableHttpResponse<?>> doFilter(HttpRequest<?> request, ServerFilterChain chain) {
        String bucket = bucketFor(request.getMethod());
        if (bucket == null) {
            return chain.proceed(request);
        }

        String clientAddress = addressResolver.resolve(request);
        if (!rateLimiter.tryAcquire(bucket, clientAddress)) {
            return Publishers.just(HttpResponse.status(HttpStatus.TOO_MANY_REQUESTS)
                    .body(new ErrorResponse("Rate limit exceeded")));
        }
        return chain.proceed(request);
    }

    private static String bucketFor(HttpMethod method) {
        return switch (method) {
            case POST -> RateLimitConfig.CREATE_BUCKET;
            case GET -> RateLimitConfig.RETRIEVE_BUCKET;
            default -> null;
        };
    }

}
