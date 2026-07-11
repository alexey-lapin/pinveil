package com.github.alexeylapin.pinveil.diceware;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class DefaultDicewareServiceTest {

    private final DefaultDicewareService diceware = new DefaultDicewareService();

    @Test
    void returnsRequestedNumberOfWords() {
        assertEquals(3, diceware.words(3).size());
        assertEquals(5, diceware.words(5).size());
    }

    @Test
    void returnsLowercaseWordsFromWordlist() {
        for (String word : diceware.words(25)) {
            assertTrue(word.matches("[a-z]+"), word);
        }
    }

}
