package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.RateLimitConfig;
import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.rate-limit.*} onto the framework-free {@link RateLimitConfig}.
 */
@ConfigurationProperties("app.rate-limit")
public class RateLimitProperties extends RateLimitConfig {

}
