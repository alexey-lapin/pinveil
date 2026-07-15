package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.MessagePolicyConfig;
import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.messages.*} onto the framework-free {@link MessagePolicyConfig}.
 */
@ConfigurationProperties("app.messages")
public class MessagePolicyProperties extends MessagePolicyConfig {

}
