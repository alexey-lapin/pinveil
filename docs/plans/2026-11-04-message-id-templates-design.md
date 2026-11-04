# Message id templates: design

Operators can choose the shape of message ids with a template, instead of the fixed
`word-word-word-NNNNNN` shape set by `message_id.words` and `message_id.digits`.

## Goals

- Support random alphanumeric ids, random numeric ids, N EFF words, and any mix of them with
  literal text, all from one template string.
- Keep today's ids as the default, so nothing changes for operators who don't set a template.
- Refuse templates that are too weak, using an entropy floor the operator controls.

## Configuration

```toml
[message_id]
template = "{word:3}-{digits:6}"   # default, same ids as today
min_bits = 32                      # default
```

Env vars: `PINVEIL_MESSAGE_ID__TEMPLATE`, `PINVEIL_MESSAGE_ID__MIN_BITS`.

`message_id.words` and `message_id.digits` are removed. Setting either one fails at startup with
"replaced by message_id.template" instead of the generic unknown-key error. The release notes map
old settings to templates: `words = W, digits = D` becomes `"{word:W}-{digits:D}"`.

## Template language

| Placeholder | Output | Bits |
|---|---|---|
| `{word}`, `{word:N}` | N EFF words joined with `-` | log2(7776) ≈ 12.925 per word |
| `{digits:N}` | N digits `0-9` | log2(10) ≈ 3.322 per char |
| `{alnum:N}` | N chars from `a-z0-9` | log2(36) ≈ 5.170 per char |
| `{Alnum:N}` | N chars from `A-Za-z0-9` | log2(62) ≈ 5.954 per char |

Examples: `msg_{alnum:8}` → `msg_k7q2x8f2`; `{word:2}.{Alnum:6}` → `brave-otter.K7q2Xf`.

Rules, all checked at startup. Each error names the problem and its byte offset in the template.

- The template input can be at most 256 bytes. This is checked before parsing, and it is a separate
  limit from the output length.
- Literal text can use only `A-Z a-z 0-9 - . _ ~`, the unreserved URL characters. `{` and `}` only
  delimit placeholders, and there is no escaping. Whitespace is not allowed anywhere.
- A placeholder is a name with an optional `:N`. `digits`, `alnum` and `Alnum` require it. N is one
  or two ASCII digits, with no sign and no leading zero, and its value is 1 to 64. Unknown names,
  extra colons and stray braces are errors.
- A template needs at least one placeholder.
- The worst-case id length can be at most 128 characters. It is computed from the validated
  wordlist's longest word, not from a sample id.
- Total bits must be at least `min_bits`, which can be 1 to 256. Bits are summed from exact log2
  values with no rounded constants. The default template has 58.706 bits.
- Startup logs the template's bits at `info`. It warns when the id space (2^bits) is smaller than
  1000 × `store.max_messages`. This is a conservative collision-headroom warning; retry exhaustion
  only becomes likely when the space is nearly full. `min_bits` is the only hard gate.

### Why summing bits is sound

No word in the EFF list is a prefix of another, so for a fixed template an id decodes left to right:
- each word draw is recovered from the left;
- literals have known content;
- digit and alnum segments have fixed lengths.

So different draws never produce the same id, even with adjacent placeholders such as
`{word:2}{word}` or words that contain a hyphen (`t-shirt`). `Wordlist::parse` enforces this by
sorting a copy and checking that no word is a prefix of the next one, and an embedded-list test
covers it too. The generator's doc comment states that it depends on this property.

## Components

- **`pinveil-core::generate`**:
  - `IdTemplate::parse(&str, &Wordlist) -> Result<IdTemplate, TemplateError>` is the only
    constructor, and the segments stay private.
  - `IdTemplate::bits()` and `IdTemplate::max_len()` report the template's strength and longest id.
  - `IdTemplate::generate(&mut impl Entropy)` builds an id.
  - Segments are `Literal(String) | Words(n) | Digits(n) | Alnum { len, upper }`.
  - Every character and word is drawn with `uniform_below`. Digits are drawn one at a time, which
    lifts the old 9-digit cap.
  - `message_id(words, digits)` is replaced.
- **`pinveil-core::wordlist`**: `Wordlist::parse` enforces the prefix-free check.
- **`pinveil-server::config`**:
  - `MessageIdConfig { template: IdTemplate, min_bits: u32 }` gets the two new keys, the old-key
    diagnostics, and the floor check.
  - The capacity warning lives here or at startup, wherever `store.max_messages` is known.
- **`pinveil-server::service`**: `WordIds` becomes `TemplateIds` and holds an `IdTemplate`. The
  `IdSource` trait and `MAX_ID_ATTEMPTS` = 10 stay as they are.
- **`pinveil-cli::links`**:
  - `ShareLink::parse` accepts ids made of `A-Z a-z 0-9 - . _ ~` and keeps their case. It rejects
    empty ids, `.` and `..`.
  - Validation uses this fixed alphabet and never the server's current template, so links to
    messages created under an older template keep working.
  - v0.1.0 CLIs reject ids with uppercase or `._~`. The release notes say that templates using them
    need an updated CLI.

Server routes, stores and the web client need no changes. They already treat ids as opaque strings.
The browser encodes them with `encodeURIComponent`, and the retrieve page HTML-escapes them.

## Documentation

- **`APP.md`:** update the "Message ids" section and the configuration table. Qualify the claim at
  line 134 that ids can't be guessed.
- **`docs/operator-guide.md:223`:** say that the bits depend on the template, describe the template
  syntax, and give both limits (256-byte input, 128-character output).
- **`docs/threat-model.md:85`, `:136`:** qualify both by template and floor. State plainly that a
  template near 32 bits can be guessed at scale: about 4.3M guesses hit some live id when 1,000
  messages are stored. The 403/404 difference also reveals which ids are live.
- **Release notes:**
  - the two keys are removed, with the mapping to templates;
  - the new CLI alphabet;
  - the old CLI incompatibility for templates outside `[a-z0-9-]`.

## Testing

- **Parser:**
  - every placeholder type, literals, adjacency, and hyphenated words;
  - exact entropy and length boundaries (just below and at `min_bits`, and at 128 characters);
  - the 256-byte input limit;
  - every malformed case (bad counts, leading zero, sign, whitespace, extra colon, stray brace,
    unknown name, illegal literal), each with its byte offset.
- **Generator:**
  - output stays within each alphabet;
  - fixed-entropy replay;
  - RNG failure propagates;
  - default-template compatibility: ids still look like `word-word-word-NNNNNN`.
- **Wordlist:** a prefix-free violation is rejected, and the embedded list passes.
- **Config:**
  - precedence for the new keys;
  - the old-key diagnostics;
  - `min_bits` range and floor rejection;
  - the capacity warning.
- **Service:** the ten-collision seam still returns 503.
- **Server contract:** create and retrieve with a template that uses uppercase and each literal
  character.
- **Migration:** a SQLite store holds messages with old-format ids, the template changes, and the old
  messages are still retrievable.
- **CLI:**
  - parsing accepts the full alphabet and preserves case;
  - the old negative fixture changes to an id with a character outside the set;
  - a retrieve round trip with an id containing uppercase, `.`, `_` and `~`.
