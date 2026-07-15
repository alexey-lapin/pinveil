package com.github.alexeylapin.pinveil.boot;

import io.micronaut.context.annotation.ConfigurationProperties;

/**
 * Binds {@code app.message-id.*}: the shape of generated message ids, as how many
 * diceware words and how many trailing digits.
 */
@ConfigurationProperties("app.message-id")
public class MessageIdProperties {

    private int words = 3;
    private int digits = 6;

    public int getWords() {
        return words;
    }

    public void setWords(int words) {
        this.words = words;
    }

    public int getDigits() {
        return digits;
    }

    public void setDigits(int digits) {
        this.digits = digits;
    }

}
