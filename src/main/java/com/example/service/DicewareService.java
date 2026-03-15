package com.example.service;

import jakarta.inject.Singleton;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.List;
import java.util.stream.Collectors;

@Singleton
public class DicewareService {
    private static final int MESSAGE_ID_WORD_COUNT = 3;
    private static final int PASSPHRASE_WORD_COUNT = 4;

    private final SecureRandom secureRandom = new SecureRandom();
    private final List<String> words;

    public DicewareService() {
        this.words = loadWords();
    }

    public String generateMessageId() {
        return String.join("-", randomWords(MESSAGE_ID_WORD_COUNT)) + "-" + String.format("%06d", secureRandom.nextInt(1_000_000));
    }

    public String generatePassphrase() {
        return String.join("-", randomWords(PASSPHRASE_WORD_COUNT));
    }

    public int wordCount() {
        return words.size();
    }

    private List<String> randomWords(int count) {
        return secureRandom.ints(count, 0, words.size())
            .mapToObj(words::get)
            .collect(Collectors.toList());
    }

    private List<String> loadWords() {
        InputStream inputStream = DicewareService.class.getResourceAsStream("/static/eff_large_wordlist.txt");
        if (inputStream == null) {
            throw new IllegalStateException("EFF wordlist is missing");
        }

        try (BufferedReader reader = new BufferedReader(new InputStreamReader(inputStream, StandardCharsets.UTF_8))) {
            List<String> loadedWords = reader.lines()
                .map(String::trim)
                .filter(line -> !line.isEmpty())
                .map(line -> line.split("\\s+", 2))
                .filter(parts -> parts.length == 2)
                .map(parts -> parts[1])
                .toList();

            if (loadedWords.size() != 7776) {
                throw new IllegalStateException("Expected 7776 diceware words but found " + loadedWords.size());
            }
            return loadedWords;
        } catch (IOException exception) {
            throw new IllegalStateException("Failed to load EFF wordlist", exception);
        }
    }
}
