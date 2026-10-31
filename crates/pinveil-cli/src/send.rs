//! `pinveil send` (APP.md, "`pinveil send [FILE...]`").

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

use pinveil_core::crypto::seal;
use pinveil_core::entropy::SystemEntropy;
use pinveil_core::envelope::blob_len;
use pinveil_core::generate::{self, is_valid_pin};
use pinveil_core::payload::{
    FileEntry, FileMeta, Payload, PayloadError, encode_payload, measure_payload,
};
use pinveil_core::wordlist::Wordlist;
use zeroize::Zeroizing;

use crate::args::SendArgs;
use crate::error::{CliError, ExitClass};
use crate::http::{Api, ServerConfig};
use crate::links::ServerUrl;
use crate::profile::{self, Profiles, parse_ttl};
use crate::{editor, store_notice, term};

/// A file named on the command line, described before it is read.
struct Picked {
    path: PathBuf,
    meta: FileMeta,
}

pub fn run(args: &SendArgs) -> Result<(), CliError> {
    let (server, ttl) = settings(args)?;
    let stdin_is_terminal = io::stdin().is_terminal();
    if args.edit && !stdin_is_terminal {
        return Err(CliError::usage(
            "--edit can't be combined with redirected stdin: send the text through one or the other",
        ));
    }
    // A generated PIN is shown on the terminal, so make sure there is one before any work.
    let mut console = if args.choose_pin {
        None
    } else {
        Some(term::console(
            "to show the generated PIN (or use --choose-pin)",
        )?)
    };
    let picked = args
        .files
        .iter()
        .map(|path| describe(path.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let text = read_text(args.edit, stdin_is_terminal, picked.is_empty())?;

    let api = Api::new(server)?;
    let config = api.config()?;
    eprintln!("{}", store_notice(&config));
    let ttl = check_ttl(ttl, &config)?;
    // Every limit is checked from metadata before any file is read.
    let size = check_size(text.as_deref().map(String::as_str), &picked, &config)?;

    let pin = if args.choose_pin {
        choose_pin()?
    } else {
        generate::pin(&mut SystemEntropy).map_err(internal)?
    };
    let payload = Payload {
        text: text.as_deref().cloned(),
        files: read_files(&picked)?,
    };
    let encoded =
        encode_payload(&payload, config.limits()).map_err(|e| payload_error(e, &config))?;
    drop(payload);
    if blob_len(encoded.len(), config.pbkdf2_iterations) != size {
        return Err(CliError::usage(
            "a file changed while it was being read; nothing was sent",
        ));
    }

    let wordlist = Wordlist::embedded().map_err(|e| internal(format!("embedded wordlist: {e}")))?;
    let passphrase = generate::passphrase(&wordlist, &mut SystemEntropy).map_err(internal)?;
    eprintln!("Encrypting {}...", bytes(size as u64));
    let blob = seal(
        &encoded,
        &passphrase,
        &pin,
        config.pbkdf2_iterations,
        &mut SystemEntropy,
    )
    .map_err(internal)?;
    drop(encoded);

    eprintln!("Uploading...");
    let created = api.create(blob, &pin, ttl)?;
    let link = api.server().share_link(&created.id, &passphrase);
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{}", link.as_str())
        .and_then(|()| stdout.flush())
        .map_err(|e| CliError::new(ExitClass::Output, format!("cannot write the link: {e}")))?;
    if let Some(console) = console.as_mut() {
        writeln!(
            console,
            "PIN: {}\nSend the PIN over a different channel than the link.",
            pin.as_str()
        )
        .map_err(|e| CliError::new(ExitClass::Output, format!("cannot show the PIN: {e}")))?;
    }
    eprintln!("Expires at {}.", created.expires_at);
    Ok(())
}

/// The server and TTL: options first, then the profile.
fn settings(args: &SendArgs) -> Result<(ServerUrl, Option<u64>), CliError> {
    let profiles = Profiles::load(&profile::config_dir()?)?;
    let profile = profiles.select(args.common.profile.as_deref())?;
    let server = args
        .server
        .clone()
        .or_else(|| profile.and_then(|p| p.server.clone()))
        .ok_or_else(|| {
            CliError::usage(
                "no server: pass --server URL, or save one with `pinveil profile set NAME --server URL --default`",
            )
        })?;
    let ttl = args
        .ttl
        .clone()
        .or_else(|| profile.and_then(|p| p.ttl.clone()));
    Ok((
        ServerUrl::parse(&server, args.common.insecure_http)?,
        ttl.as_deref().map(parse_ttl).transpose()?,
    ))
}

/// The text: redirected stdin, the editor, or a prompt when there are no files. Empty text
/// means none.
fn read_text(
    edit: bool,
    stdin_is_terminal: bool,
    no_files: bool,
) -> Result<Option<Zeroizing<String>>, CliError> {
    let text = if !stdin_is_terminal {
        read_stdin()?
    } else if edit {
        editor::edit()?
    } else if no_files {
        eprintln!("Type the message, then press Ctrl-D (on Windows: Ctrl-Z, then Enter).");
        read_stdin()?
    } else {
        Zeroizing::new(String::new())
    };
    Ok((!text.is_empty()).then_some(text))
}

/// The exact encrypted size, from the text and the files' metadata. Refuses a message the
/// server would refuse.
fn check_size(
    text: Option<&str>,
    picked: &[Picked],
    config: &ServerConfig,
) -> Result<usize, CliError> {
    let metas: Vec<FileMeta> = picked.iter().map(|p| p.meta.clone()).collect();
    let measured =
        measure_payload(text, &metas, config.limits()).map_err(|e| payload_error(e, config))?;
    let size =
        usize::try_from(measured).map_or(usize::MAX, |len| blob_len(len, config.pbkdf2_iterations));
    if size as u64 > config.max_payload_bytes {
        return Err(CliError::usage(format!(
            "this message is {} once encrypted; the server accepts up to {}",
            bytes(size as u64),
            bytes(config.max_payload_bytes)
        )));
    }
    Ok(size)
}

fn read_files(picked: &[Picked]) -> Result<Vec<FileEntry>, CliError> {
    picked
        .iter()
        .map(|item| {
            let data = fs::read(&item.path).map_err(|e| {
                CliError::usage(format!("cannot read {}: {e}", item.path.display()))
            })?;
            if data.len() as u64 != item.meta.byte_length {
                return Err(CliError::usage(format!(
                    "{} changed while it was being read; nothing was sent",
                    item.path.display()
                )));
            }
            Ok(FileEntry {
                name: item.meta.name.clone(),
                mime_type: item.meta.mime_type.clone(),
                data,
            })
        })
        .collect()
}

fn describe(path: PathBuf) -> Result<Picked, CliError> {
    let metadata = fs::metadata(&path)
        .map_err(|e| CliError::usage(format!("cannot read {}: {e}", path.display())))?;
    if !metadata.is_file() {
        return Err(CliError::usage(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            CliError::usage(format!(
                "the name of {} is not valid UTF-8; rename the file",
                path.display()
            ))
        })?
        .to_owned();
    let mime_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned();
    Ok(Picked {
        meta: FileMeta {
            name,
            mime_type,
            byte_length: metadata.len(),
        },
        path,
    })
}

fn read_stdin() -> Result<Zeroizing<String>, CliError> {
    let mut bytes = Zeroizing::new(Vec::new());
    io::stdin()
        .lock()
        .read_to_end(&mut bytes)
        .map_err(|e| CliError::usage(format!("cannot read stdin: {e}")))?;
    String::from_utf8(bytes.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| {
            CliError::usage("stdin is not UTF-8 text; send binary data as a file argument instead")
        })
}

fn check_ttl(ttl: Option<u64>, config: &ServerConfig) -> Result<u64, CliError> {
    let ttl = ttl.unwrap_or(config.default_ttl_seconds);
    if (config.min_ttl_seconds..=config.max_ttl_seconds).contains(&ttl) {
        Ok(ttl)
    } else {
        Err(CliError::usage(format!(
            "this server accepts a TTL from {}s to {}s",
            config.min_ttl_seconds, config.max_ttl_seconds
        )))
    }
}

fn choose_pin() -> Result<Zeroizing<String>, CliError> {
    for _ in 0..3 {
        let pin = term::prompt_hidden("PIN (6 digits): ", "for the PIN prompt")?;
        if is_valid_pin(&pin) {
            return Ok(pin);
        }
        eprintln!("The PIN must be exactly six digits.");
    }
    Err(CliError::usage("no valid PIN was entered"))
}

fn payload_error(error: PayloadError, config: &ServerConfig) -> CliError {
    CliError::usage(match error {
        PayloadError::Empty => {
            "nothing to send: pipe text in, type it at the prompt, or name files".to_owned()
        }
        PayloadError::TooManyFiles => {
            format!(
                "this server accepts up to {} files per message",
                config.max_files
            )
        }
        PayloadError::ManifestTooLarge => {
            "the file names take up too much space; use fewer files or shorter names".to_owned()
        }
        PayloadError::BadName => "a file name is longer than 255 bytes; rename it".to_owned(),
        other => other.to_string(),
    })
}

fn internal(error: impl std::fmt::Display) -> CliError {
    CliError::new(ExitClass::Internal, error.to_string())
}

#[allow(clippy::cast_precision_loss)]
fn bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KiB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", n as f64 / (1024.0 * 1024.0))
    }
}
