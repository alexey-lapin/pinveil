package com.github.alexeylapin.pinveil.diceware;

import com.github.alexeylapin.pinveil.config.MessageIdConfig;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertTrue;

class DicewareIdGeneratorTest {

    private final DicewareService diceware = new DefaultDicewareService();

    @Test
    void generatesThreeWordsAndSixDigitsByDefault() {
        DicewareIdGenerator generator = new DicewareIdGenerator(diceware, new MessageIdConfig());
        assertTrue(generator.get().matches("^[a-z]+-[a-z]+-[a-z]+-\\d{6}$"), generator.get());
    }

    @Test
    void honoursConfiguredWordCountAndDigits() {
        MessageIdConfig config = new MessageIdConfig();
        config.setWords(2);
        config.setDigits(4);
        DicewareIdGenerator generator = new DicewareIdGenerator(diceware, config);
        assertTrue(generator.get().matches("^[a-z]+-[a-z]+-\\d{4}$"), generator.get());
    }

}
