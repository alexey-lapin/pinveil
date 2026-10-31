# Assets

`eff_large_wordlist.txt` is the EFF Large Wordlist for passphrases (7776 words, one
`dice-roll<TAB>word` entry per line).

- Source: https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt
- Announcement: https://www.eff.org/deeplinks/2016/07/new-wordlists-random-passphrases
- Published by the Electronic Frontier Foundation (EFF), 2016-07-18
- License: EFF's copyright policy (https://www.eff.org/copyright), which licenses EFF's original
  material under Creative Commons Attribution 4.0 International (CC BY 4.0) unless otherwise noted.
  The file itself carries no license header.

Both the server and the CLI embed this file, and the browser fetches it from the server.
Every consumer validates it before use (APP.md invariant 12).
