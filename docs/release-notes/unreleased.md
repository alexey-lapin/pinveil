## Configurable message ids

`message_id.template` sets the shape of message ids, with placeholders for EFF words
(`{word:N}`), digits (`{digits:N}`), and letters and digits (`{alnum:N}`, `{Alnum:N}`), plus
literal text. The default, `{word:3}-{digits:6}`, gives the same ids as before.
`message_id.min_bits` (default 32) stops the server from starting with a template that is too
easy to guess. See the operator guide, "Message ids".

## Breaking changes

- `message_id.words` and `message_id.digits` were removed, and setting either one now stops
  the server with a pointer to the new key. Replace `words = W` and `digits = D` with
  `template = "{word:W}-{digits:D}"`. If you never set them, there is nothing to do.
- The `pinveil` CLI now accepts ids with uppercase letters and `. _ ~`. The 0.1.0 CLI rejects
  those links, so upgrade it before you switch to a template that produces them. The default
  template works with both.
- A wordlist in which one word is a prefix of another is now refused. The embedded EFF list
  passes.
