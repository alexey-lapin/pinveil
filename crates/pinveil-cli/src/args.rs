//! Command-line parsing with `usage-lib` (APP.md, "CLI").
//!
//! The spec below is the single source of truth for commands and flags. Unknown flags and
//! repeated single-value flags are errors. Content and PIN never pass through here. The one
//! secret that can, the fragment of `get`'s opt-in positional URL, is cut off before the
//! parser sees the arguments, so no usage error or help text can echo it.

use std::fmt::Write;
use std::path::PathBuf;

use usage::parse::{ParseOutput, ParseValue, TokenRole};
use usage::{Spec, SpecCommand};
use zeroize::Zeroizing;

use crate::error::CliError;

const SPEC: &str = r#"
name "pinveil"
bin "pinveil"
about "Zero-knowledge one-time secret sharing from the terminal"
unknown_flags "error"

cmd "send" help="Encrypt text and files, upload them, and print a one-time link" {
    long_help "Text comes from stdin when it is redirected. On a terminal, pinveil asks for it, or opens $EDITOR with --edit. With files and no redirected stdin, the message holds only the files. The PIN is generated and shown on the terminal; send it over a different channel than the link."
    flag "--ttl <duration>" help="How long the message lives, such as 15m, 2h, or 1d (default: the server's default)"
    flag "--server <url>" help="Server URL (overrides the profile)"
    flag "--profile <name>" help="Profile to use (default: the default profile, if any)"
    flag "--edit" help="Write the text in $VISUAL or $EDITOR"
    flag "--choose-pin" help="Type the PIN at a hidden prompt instead of generating one"
    flag "--insecure-http" help="Allow plain HTTP to a loopback IP address (127.0.0.0/8 or [::1]) for local testing"
    arg "[file]..." help="Files to attach" var=#true
}

cmd "get" help="Download, decrypt, and save a message" {
    long_help "By default pinveil asks for the link at a hidden prompt, then for the PIN. Text goes to stdout; files go to the current directory or --out."
    flag "--out <dir>" help="Directory for files (overrides the profile)"
    flag "--force" help="Replace existing files"
    flag "--raw" help="Write text to a terminal unchanged, control sequences included"
    flag "--profile <name>" help="Profile to use for the output directory"
    flag "--insecure-http" help="Allow plain HTTP to a loopback IP address (127.0.0.0/8 or [::1]) for local testing"
    arg "[url]" help="The share link. Exposes the passphrase in shell history and the process list; prefer the prompt"
}

cmd "profile" help="Manage server profiles (non-secret settings only)" subcommand_required=#true {
    cmd "list" help="List profiles"
    cmd "show" help="Show one profile" {
        arg "<name>"
    }
    cmd "set" help="Create or update a profile" {
        flag "--server <url>" help="Server URL"
        flag "--ttl <duration>" help="Default TTL for send"
        flag "--out <dir>" help="Default output directory for get"
        flag "--default" help="Make this the default profile"
        arg "<name>"
    }
    cmd "remove" help="Delete a profile" {
        arg "<name>"
    }
}
"#;

/// Where an option value came from, for flags that may also come from a profile.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Common {
    pub profile: Option<String>,
    pub insecure_http: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendArgs {
    pub common: Common,
    pub ttl: Option<String>,
    pub server: Option<String>,
    pub edit: bool,
    pub choose_pin: bool,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetArgs {
    pub common: Common,
    pub out: Option<PathBuf>,
    pub force: bool,
    pub raw: bool,
    /// The opt-in positional link, fragment included. Wiped on drop.
    pub url: Option<Zeroizing<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileArgs {
    List,
    Show(String),
    Set {
        name: String,
        server: Option<String>,
        ttl: Option<String>,
        out: Option<PathBuf>,
        default: bool,
    },
    Remove(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Send(SendArgs),
    Get(GetArgs),
    Profile(ProfileArgs),
    Version,
    /// Help text to print to stdout.
    Help(String),
}

/// Parses `argv` (program name first).
pub fn parse(argv: &[String]) -> Result<Command, CliError> {
    let spec: Spec = SPEC.parse().expect("the embedded command spec is valid");
    if argv.len() == 2 && matches!(argv[1].as_str(), "--version" | "-V") {
        return Ok(Command::Version);
    }
    if argv.len() < 2 {
        return Err(CliError::usage(top_help(&spec)));
    }
    // Help is answered before parsing, from the spec alone: it never echoes an argument.
    let words: Vec<&str> = argv[1..].iter().map(String::as_str).collect();
    if asks_for_help(&words) {
        return Ok(Command::Help(help_for(&spec, &words)));
    }

    // Every word holding a `#` reaches the parser with what follows it hidden, so no usage
    // error can echo a passphrase, even from a malformed link. Values are read back from the
    // original words by position afterwards, so `notes#1.txt` still works.
    let redacted: Vec<String> = argv
        .iter()
        .map(|word| match word.split_once('#') {
            Some((before, _)) => format!("{before}#<hidden>"),
            None => word.clone(),
        })
        .collect();

    let parsed =
        usage::parse(&spec, &redacted).map_err(|report| CliError::usage(report.to_string()))?;
    reject_repeated_flags(&parsed)?;
    let values = Values {
        parsed: &parsed,
        argv,
    };

    let names: Vec<&str> = parsed.cmds.iter().map(|c| c.name.as_str()).collect();
    match names.as_slice() {
        [_, "send"] => Ok(Command::Send(SendArgs {
            common: values.common(),
            ttl: values.flag("ttl"),
            server: values.flag("server"),
            edit: bool_flag(&parsed, "edit"),
            choose_pin: bool_flag(&parsed, "choose-pin"),
            files: values.args("file").into_iter().map(PathBuf::from).collect(),
        })),
        [_, "get"] => Ok(Command::Get(GetArgs {
            common: values.common(),
            out: values.flag("out").map(PathBuf::from),
            force: bool_flag(&parsed, "force"),
            raw: bool_flag(&parsed, "raw"),
            url: values.args("url").into_iter().next().map(Zeroizing::new),
        })),
        [_, "profile", "list"] => Ok(Command::Profile(ProfileArgs::List)),
        [_, "profile", "show"] => Ok(Command::Profile(ProfileArgs::Show(values.name()))),
        [_, "profile", "remove"] => Ok(Command::Profile(ProfileArgs::Remove(values.name()))),
        [_, "profile", "set"] => Ok(Command::Profile(ProfileArgs::Set {
            name: values.name(),
            server: values.flag("server"),
            ttl: values.flag("ttl"),
            out: values.flag("out").map(PathBuf::from),
            default: bool_flag(&parsed, "default"),
        })),
        _ => Err(CliError::usage(top_help(&spec))),
    }
}

/// Flags that take a value: the word after one is that value, never a help request.
const VALUE_FLAGS: &[&str] = &["--ttl", "--server", "--profile", "--out"];

/// Whether `--help` or `-h` appears as an option, not as a flag's value, and before `--`.
fn asks_for_help(words: &[&str]) -> bool {
    let mut words = words.iter();
    while let Some(word) = words.next() {
        match *word {
            "--" => return false,
            "--help" | "-h" => return true,
            flag if VALUE_FLAGS.contains(&flag) => {
                words.next();
            }
            _ => {}
        }
    }
    false
}

/// Reads option and argument values from the original words, by the positions the parser
/// bound them to, so values come back exactly as typed (fragments included).
struct Values<'a> {
    parsed: &'a ParseOutput,
    argv: &'a [String],
}

impl Values<'_> {
    fn common(&self) -> Common {
        Common {
            profile: self.flag("profile"),
            insecure_http: bool_flag(self.parsed, "insecure-http"),
        }
    }

    fn flag(&self, name: &str) -> Option<String> {
        self.parsed.tokens.iter().find_map(|token| {
            token.roles.iter().find_map(|role| match role {
                TokenRole::Value { flag, attached, .. } if flag.name == name => {
                    let word = &self.argv[token.index];
                    Some(if *attached {
                        word.split_once('=')
                            .map_or(word.as_str(), |(_, v)| v)
                            .to_owned()
                    } else {
                        word.clone()
                    })
                }
                _ => None,
            })
        })
    }

    fn args(&self, name: &str) -> Vec<String> {
        self.parsed
            .tokens
            .iter()
            .filter(|token| {
                token
                    .roles
                    .iter()
                    .any(|role| matches!(role, TokenRole::Arg { arg, .. } if arg.name == name))
            })
            .map(|token| self.argv[token.index].clone())
            .collect()
    }

    fn name(&self) -> String {
        self.args("name")
            .into_iter()
            .next()
            .expect("<name> is required by the spec")
    }
}

/// usage-lib keeps the last value of a repeated flag. Every flag here is single-valued, and a
/// repeat is far more likely a mistake than an override, so it is an error.
fn reject_repeated_flags(parsed: &ParseOutput) -> Result<(), CliError> {
    let mut seen: Vec<&str> = Vec::new();
    for token in &parsed.tokens {
        for role in &token.roles {
            if let TokenRole::Flag { flag, .. } = role {
                if seen.contains(&flag.name.as_str()) {
                    return Err(CliError::usage(format!(
                        "--{} was given more than once",
                        flag.name
                    )));
                }
                seen.push(flag.name.as_str());
            }
        }
    }
    Ok(())
}

fn top_help(spec: &Spec) -> String {
    format!(
        "usage: pinveil <command> [options]\n\ncommands:\n{}\n\nRun `pinveil <command> --help` for details.",
        subcommand_lines(&spec.cmd)
    )
}

fn subcommand_lines(cmd: &SpecCommand) -> String {
    cmd.subcommands
        .values()
        .map(|sub| format!("  {:<8} {}", sub.name, sub.help.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Help for the deepest command named in `words`.
fn help_for(spec: &Spec, words: &[&str]) -> String {
    let mut cmd = &spec.cmd;
    let mut path = vec!["pinveil"];
    for word in words {
        if let Some(sub) = cmd.subcommands.get(*word) {
            path.push(sub.name.as_str());
            cmd = sub;
        }
    }
    if path.len() == 1 {
        return top_help(spec);
    }
    let mut out = format!("usage: {}", path.join(" "));
    if !cmd.subcommands.is_empty() {
        out.push_str(" <command>");
    }
    if !cmd.flags.is_empty() {
        out.push_str(" [options]");
    }
    for arg in &cmd.args {
        out.push(' ');
        out.push_str(&arg.usage);
    }
    for text in [&cmd.help, &cmd.help_long].into_iter().flatten() {
        out.push_str("\n\n");
        out.push_str(text);
    }
    if !cmd.subcommands.is_empty() {
        out.push_str("\n\ncommands:\n");
        out.push_str(&subcommand_lines(cmd));
    }
    if !cmd.args.is_empty() {
        out.push_str("\n\narguments:");
        for arg in &cmd.args {
            let help = arg.help.clone().unwrap_or_default();
            let _ = write!(out, "\n  {:<20} {help}", arg.usage);
        }
    }
    if !cmd.flags.is_empty() {
        out.push_str("\n\noptions:");
        for flag in &cmd.flags {
            let help = flag.help.clone().unwrap_or_default();
            let _ = write!(out, "\n  {:<20} {help}", flag.usage);
        }
    }
    out
}

fn flag_value<'a>(parsed: &'a ParseOutput, name: &str) -> Option<&'a ParseValue> {
    parsed
        .flags
        .iter()
        .find(|(flag, _)| flag.name == name)
        .map(|(_, value)| value)
}

fn bool_flag(parsed: &ParseOutput, name: &str) -> bool {
    matches!(flag_value(parsed, name), Some(ParseValue::Bool(true)))
}

#[cfg(test)]
mod tests {
    use super::{Command, ProfileArgs, parse};
    use crate::error::ExitClass;

    fn argv(words: &[&str]) -> Vec<String> {
        std::iter::once("pinveil")
            .chain(words.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    fn send(words: &[&str]) -> super::SendArgs {
        match parse(&argv(&[&["send"], words].concat())).unwrap() {
            Command::Send(args) => args,
            other => panic!("expected send, got {other:?}"),
        }
    }

    #[test]
    fn cli_parser_send_flags_and_files() {
        let args = send(&[
            "--ttl",
            "2h",
            "--server",
            "https://example.org",
            "--choose-pin",
            "a.txt",
            "dir/b.bin",
        ]);
        assert_eq!(args.ttl.as_deref(), Some("2h"));
        assert_eq!(args.server.as_deref(), Some("https://example.org"));
        assert!(args.choose_pin);
        assert!(!args.edit);
        assert_eq!(args.files.len(), 2);
        assert!(send(&[]).files.is_empty());
        assert_eq!(
            send(&["notes#1.txt"]).files[0].to_str(),
            Some("notes#1.txt")
        );
        assert_eq!(send(&["--ttl=1h", "a#b"]).ttl.as_deref(), Some("1h"));
        assert_eq!(
            send(&["--server=https://h/x#y"]).server.as_deref(),
            Some("https://h/x#y")
        );
    }

    #[test]
    fn cli_parser_get_and_profile_commands() {
        let Command::Get(get) = parse(&argv(&["get", "--out", "x", "--force", "--raw"])).unwrap()
        else {
            panic!()
        };
        assert_eq!(get.out.as_deref(), Some(std::path::Path::new("x")));
        assert!(get.force && get.raw && get.url.is_none());
        assert_eq!(
            parse(&argv(&[
                "profile",
                "set",
                "work",
                "--server",
                "https://a",
                "--default"
            ]))
            .unwrap(),
            Command::Profile(ProfileArgs::Set {
                name: "work".into(),
                server: Some("https://a".into()),
                ttl: None,
                out: None,
                default: true,
            })
        );
        assert_eq!(
            parse(&argv(&["profile", "list"])).unwrap(),
            Command::Profile(ProfileArgs::List)
        );
        assert_eq!(parse(&argv(&["--version"])).unwrap(), Command::Version);
    }

    #[test]
    fn cli_parser_unknown_and_repeated_flags_are_usage_errors() {
        for words in [
            &["send", "--pin", "123456"][..],
            &["send", "--ttl", "1h", "--ttl", "2h"],
            &["get", "--out", "a", "--out", "b"],
            &["send", "--text", "secret"],
            &["frobnicate"],
            &["profile"],
            &["get", "a", "b"],
        ] {
            let error = parse(&argv(words)).unwrap_err();
            assert_eq!(error.class, ExitClass::Usage, "{words:?}");
        }
    }

    #[test]
    fn inv_2_parser_diagnostics_redacted() {
        let url = "https://pinveil.example/message/a-b-c-123456#cloak-tidal-barcode-unbalance";
        // A usage error on a command line holding the link never shows the passphrase.
        for words in [
            &["get", url, "--bogus"][..],
            &["get", url, "extra"],
            &["get", "--out", url, "--out", "x"],
            &["send", url, "--ttl"],
        ] {
            let error = parse(&argv(words)).unwrap_err();
            assert!(
                !error.message.contains("cloak") && !error.message.contains("unbalance"),
                "{words:?}: {}",
                error.message
            );
        }
        // Malformed links and stray words with a fragment are hidden too.
        for words in [
            &[
                "get",
                "output-dir",
                "https:/example.test/message/id#SENTINEL_SECRET",
            ][..],
            &["get", "out", "//example.test/message/id#SENTINEL_SECRET"],
            &["get", "a", "b#SENTINEL_SECRET"],
            &["send", "--nope", "x#SENTINEL_SECRET"],
            &["profile", "show", "a#SENTINEL_SECRET", "extra"],
        ] {
            let error = parse(&argv(words)).unwrap_err();
            assert!(
                !error.message.contains("SENTINEL"),
                "{words:?}: {}",
                error.message
            );
        }
        // The parsed value keeps the fragment.
        let Command::Get(get) = parse(&argv(&["get", url])).unwrap() else {
            panic!()
        };
        assert_eq!(get.url.as_deref().map(String::as_str), Some(url));
    }

    #[test]
    fn cli_parser_help_is_not_an_error() {
        assert!(matches!(
            parse(&argv(&["send", "--help"])).unwrap(),
            Command::Help(text) if text.contains("--choose-pin") && text.starts_with("usage: pinveil send")
        ));
        let url = "https://h/message/x-1#secret-words-here-now";
        assert!(matches!(
            parse(&argv(&["get", url, "-h"])).unwrap(),
            Command::Help(text) if text.contains("shell history") && !text.contains("secret")
        ));
        assert!(matches!(
            parse(&argv(&["profile", "--help"])).unwrap(),
            Command::Help(text) if text.contains("remove")
        ));
        // A value or file named like a help word is just a value.
        let Command::Get(get) = parse(&argv(&["get", "--out", "help"])).unwrap() else {
            panic!()
        };
        assert_eq!(get.out.as_deref(), Some(std::path::Path::new("help")));
        let Command::Get(get) = parse(&argv(&["get", "--out=-h"])).unwrap() else {
            panic!()
        };
        assert_eq!(get.out.as_deref(), Some(std::path::Path::new("-h")));
        assert_eq!(send(&["help"]).files[0].to_str(), Some("help"));
        assert!(matches!(
            parse(&argv(&["profile", "set", "help", "--ttl", "1h"])).unwrap(),
            Command::Profile(ProfileArgs::Set { name, .. }) if name == "help"
        ));
        assert!(matches!(
            parse(&argv(&["send", "-h"])).unwrap(),
            Command::Help(_)
        ));
        assert_eq!(parse(&argv(&[])).unwrap_err().class, ExitClass::Usage);
    }
}
