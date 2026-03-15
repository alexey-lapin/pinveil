package com.example.service;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class DicewareServiceTest {
    private final DicewareService dicewareService = new DicewareService();

    @Test
    void loadsFullEffWordList() {
        assertEquals(7776, dicewareService.wordCount());
    }

    @Test
    void generatesMessageIdsInExpectedFormat() {
        assertTrue(dicewareService.generateMessageId().matches("^[a-z]+-[a-z]+-[a-z]+-\\d{6}$"));
    }

    @Test
    void generatesPassphrasesInExpectedFormat() {
        assertTrue(dicewareService.generatePassphrase().matches("^[a-z]+-[a-z]+-[a-z]+-[a-z]+$"));
    }
}
