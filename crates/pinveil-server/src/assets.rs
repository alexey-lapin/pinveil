//! Browser assets embedded in the binary: the pages, `web/static`, and the wordlist.

/// One embedded static file.
pub struct Asset {
    pub name: &'static str,
    pub content_type: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! web_static {
    ($name:literal, $content_type:expr) => {
        Asset {
            name: $name,
            content_type: $content_type,
            bytes: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../web/static/",
                $name
            )),
        }
    };
}

const JS: &str = "text/javascript; charset=utf-8";

/// Everything served under `/static/`. A test checks this matches `web/static` exactly.
pub static STATIC: &[Asset] = &[
    web_static!("app.css", "text/css; charset=utf-8"),
    web_static!("app.js", JS),
    web_static!("config.js", JS),
    web_static!("create-page.js", JS),
    web_static!("crypto.js", JS),
    web_static!("envelope.js", JS),
    web_static!("export-names.js", JS),
    web_static!("payload.js", JS),
    web_static!("protocol.js", JS),
    web_static!("random.js", JS),
    web_static!("retrieve-page.js", JS),
    web_static!("ui.js", JS),
    web_static!("wordlist.js", JS),
    Asset {
        name: "eff_large_wordlist.txt",
        content_type: "text/plain; charset=utf-8",
        bytes: pinveil_core::wordlist::EMBEDDED_TEXT.as_bytes(),
    },
];

/// The create page template, with `{{client_config_json}}`.
pub const INDEX_HTML: &str = include_str!("../../../web/pages/index.html");
/// The retrieve page template, with `{{client_config_json}}` and `{{message_id}}`.
pub const MESSAGE_HTML: &str = include_str!("../../../web/pages/message.html");

#[must_use]
pub fn find(name: &str) -> Option<&'static Asset> {
    STATIC.iter().find(|asset| asset.name == name)
}

/// Escapes text for an HTML attribute or element body.
#[must_use]
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Makes JSON safe inside `<script type="application/json">`: `<`, `>`, and `&` only occur
/// inside JSON strings, where `<`-style escapes mean the same thing.
#[must_use]
pub fn script_safe_json(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

#[cfg(test)]
mod tests {
    use super::{JS, STATIC, html_escape, script_safe_json};

    #[test]
    fn every_web_static_file_is_embedded() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/static");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        on_disk.push("eff_large_wordlist.txt".into());
        on_disk.sort();
        let mut embedded: Vec<String> = STATIC.iter().map(|a| a.name.to_owned()).collect();
        embedded.sort();
        assert_eq!(embedded, on_disk);
        assert!(
            STATIC
                .iter()
                .filter(|a| std::path::Path::new(a.name)
                    .extension()
                    .is_some_and(|e| e == "js"))
                .all(|a| a.content_type == JS)
        );
    }

    #[test]
    fn escaping() {
        assert_eq!(html_escape(r#"a"b<c>&'d"#), "a&quot;b&lt;c&gt;&amp;&#39;d");
        assert_eq!(
            script_safe_json(r#"{"x":"</script><b>&"}"#),
            r#"{"x":"\u003c/script\u003e\u003cb\u003e\u0026"}"#
        );
    }
}
