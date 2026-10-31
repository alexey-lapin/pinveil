/**
 * The EFF large wordlist, fetched from the server and validated before use (APP.md
 * invariant 12). The rules match `pinveil-core`'s `Wordlist::parse`.
 */

export const WORDLIST_LEN = 7776;

const DICE_CODE = /^[1-6]{5}$/;
const WORD = /^[a-z]+(?:-[a-z]+)*$/;

/** Parses `dice<TAB>word` lines and throws unless the list is complete and valid. */
export function parseWordList(text) {
    const body = text.endsWith("\n") ? text.slice(0, -1) : text;
    const words = [];
    const seenCodes = new Set();
    const seenWords = new Set();
    for (const [index, line] of body.split("\n").entries()) {
        const tab = line.indexOf("\t");
        const code = tab === -1 ? "" : line.slice(0, tab);
        const word = tab === -1 ? "" : line.slice(tab + 1);
        if (!DICE_CODE.test(code) || !WORD.test(word)) {
            throw new Error(`Wordlist line ${index + 1} is malformed.`);
        }
        if (seenCodes.has(code) || seenWords.has(word)) {
            throw new Error(`Wordlist line ${index + 1} repeats a dice code or word.`);
        }
        seenCodes.add(code);
        seenWords.add(word);
        words.push(word);
    }
    if (words.length !== WORDLIST_LEN) {
        throw new Error(`Wordlist has ${words.length} entries, expected ${WORDLIST_LEN}.`);
    }
    return words;
}

let wordListPromise;

/**
 * Fetches and validates the wordlist once per page. A failure is not cached, so a later
 * attempt fetches again.
 */
export function loadWordList(fetchImpl = fetch) {
    if (!wordListPromise) {
        wordListPromise = fetchWordList(fetchImpl).catch((error) => {
            wordListPromise = undefined;
            throw error;
        });
    }
    return wordListPromise;
}

async function fetchWordList(fetchImpl) {
    const response = await fetchImpl("/static/eff_large_wordlist.txt", { cache: "force-cache" });
    if (!response.ok) {
        throw new Error(`Could not load the wordlist (HTTP ${response.status}).`);
    }
    return parseWordList(await response.text());
}
