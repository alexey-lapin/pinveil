package com.github.alexeylapin.pinveil.diceware;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertTrue;

class DicewareIdGeneratorTest {

    private final DicewareService diceware = new DefaultDicewareService();

    @Test
    void generatesThreeWordsAndSixDigits() {
        DicewareIdGenerator generator = new DicewareIdGenerator(diceware, 3, 6);
        assertTrue(generator.get().matches("^[a-z]+-[a-z]+-[a-z]+-\\d{6}$"), generator.get());
    }

    @Test
    void honoursConfiguredWordCountAndDigits() {
        DicewareIdGenerator generator = new DicewareIdGenerator(diceware, 2, 4);
        assertTrue(generator.get().matches("^[a-z]+-[a-z]+-\\d{4}$"), generator.get());
    }

}
