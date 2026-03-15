package com.example.config;

import io.micronaut.http.MutableHttpResponse;
import io.micronaut.http.annotation.Filter;
import io.micronaut.core.async.publisher.Publishers;
import io.micronaut.http.filter.HttpServerFilter;
import io.micronaut.http.filter.ServerFilterChain;
import io.micronaut.http.HttpRequest;
import org.reactivestreams.Publisher;

@Filter("/**")
public class SecurityHeadersFilter implements HttpServerFilter {
    private static final String CONTENT_SECURITY_POLICY = String.join("; ",
        "default-src 'self'",
        "script-src 'self'",
        "style-src 'self'",
        "img-src 'self' data:",
        "font-src 'self'",
        "connect-src 'self'",
        "base-uri 'none'",
        "frame-ancestors 'none'",
        "form-action 'self'"
    );

    @Override
    public Publisher<MutableHttpResponse<?>> doFilter(HttpRequest<?> request, ServerFilterChain chain) {
        return Publishers.map(chain.proceed(request), response -> {
                response.header("Cache-Control", "no-store, max-age=0");
                response.header("Pragma", "no-cache");
                response.header("Referrer-Policy", "no-referrer");
                response.header("Content-Security-Policy", CONTENT_SECURITY_POLICY);
                response.header("X-Content-Type-Options", "nosniff");
                response.header("X-Frame-Options", "DENY");
                return response;
            });
    }
}
