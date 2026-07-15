package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;
import io.micronaut.core.bind.annotation.Bindable;

/**
 * Binds {@code app.message-id.*}: the shape of generated message ids, as how many
 * diceware words and how many trailing digits.
 */
@ConfigurationProperties("app.message-id")
public interface MessageIdProperties {

    @Bindable(defaultValue = "3")
    int getWords();

    @Bindable(defaultValue = "6")
    int getDigits();

}
