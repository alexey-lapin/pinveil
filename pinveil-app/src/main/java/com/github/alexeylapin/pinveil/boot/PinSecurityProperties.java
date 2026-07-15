package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.PinSecurityConfig;
import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.pin.*} onto the framework-free {@link PinSecurityConfig}.
 */
@ConfigurationProperties("app.pin")
public class PinSecurityProperties extends PinSecurityConfig {

}
