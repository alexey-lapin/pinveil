/** Lazily fetches and caches the EFF wordlist, shared across calls in this module. */

let wordListPromise;

export function loadWordList() {
    if (!wordListPromise) {
        wordListPromise = fetch("/static/eff_large_wordlist.txt", { cache: "force-cache" })
            .then((response) => response.text())
            .then((text) => text.trim().split(/\n+/).map((line) => line.trim().split(/\s+/, 2)[1]));
    }
    return wordListPromise;
}
