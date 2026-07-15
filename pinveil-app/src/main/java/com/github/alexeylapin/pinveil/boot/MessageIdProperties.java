package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.MessageIdConfig;
import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.message-id.*} onto the framework-free {@link MessageIdConfig}.
 */
@ConfigurationProperties("app.message-id")
public class MessageIdProperties extends MessageIdConfig {

}
