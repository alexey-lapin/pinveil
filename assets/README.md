# Assets

`eff_large_wordlist.txt` is the EFF Large Wordlist for passphrases (7776 words, one
`dice-roll<TAB>word` entry per line).

- Source: https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt
- Published by the Electronic Frontier Foundation, 2016-07-18
- License: Creative Commons Attribution 3.0 United States (CC BY 3.0 US)

Both the server and the CLI embed this file, and the browser fetches it from the server.
Every consumer validates it before use (APP.md invariant 12).
