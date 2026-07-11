package com.github.alexeylapin.pinveil.diceware;

import java.util.List;

/**
 * Provides random words drawn from a wordlist.
 */
public interface DicewareService {

    /**
     * @param count how many words to return.
     * @return {@code count} words chosen uniformly at random (with replacement) from the wordlist.
     */
    List<String> words(int count);

}
