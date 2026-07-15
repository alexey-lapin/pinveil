package com.github.alexeylapin.pinveil.boot;

import com.github.alexeylapin.pinveil.config.InMemoryStoreConfig;
import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.store.in-memory.*} onto the framework-free {@link InMemoryStoreConfig}.
 */
@ConfigurationProperties("app.store.in-memory")
public class InMemoryStoreProperties extends InMemoryStoreConfig {

}
