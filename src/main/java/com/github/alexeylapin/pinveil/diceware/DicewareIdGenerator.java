package com.github.alexeylapin.pinveil.diceware;

import com.github.alexeylapin.pinveil.config.MessageIdConfig;
import jakarta.inject.Singleton;

import java.security.SecureRandom;
import java.util.function.Supplier;

/**
 * Supplies human-friendly message ids: a configurable number of random diceware
 * words followed by a zero-padded random number, e.g.
 * {@code aflame-universal-tidal-152634}.
 */
@Singleton
public class DicewareIdGenerator implements Supplier<String> {

    private final DicewareService diceware;
    private final int wordCount;
    private final int digits;
    private final int numberBound;
    private final SecureRandom secureRandom = new SecureRandom();

    public DicewareIdGenerator(DicewareService diceware, MessageIdConfig config) {
        this.diceware = diceware;
        this.wordCount = config.getWords();
        this.digits = config.getDigits();
        this.numberBound = pow10(config.getDigits());
    }

    @Override
    public String get() {
        String words = String.join("-", diceware.words(wordCount));
        String number = String.format("%0" + digits + "d", secureRandom.nextInt(numberBound));
        return words + "-" + number;
    }

    private static int pow10(int exponent) {
        int result = 1;
        for (int i = 0; i < exponent; i++) {
            result *= 10;
        }
        return result;
    }

}
