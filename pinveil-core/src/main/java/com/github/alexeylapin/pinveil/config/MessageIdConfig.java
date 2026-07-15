package com.github.alexeylapin.pinveil.config;

/**
 * Shape of generated message ids: how many diceware words and how many trailing digits.
 */
public class MessageIdConfig {

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
