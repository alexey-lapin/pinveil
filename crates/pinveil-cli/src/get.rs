//! `pinveil get` (APP.md, "`pinveil get [URL]`").

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use pinveil_core::crypto::open_payload;
use pinveil_core::generate::is_valid_pin;

use crate::args::GetArgs;
use crate::display::neutralize;
use crate::error::{CliError, ExitClass};
use crate::http::{Api, Retrieved};
use crate::links::ShareLink;
use crate::output::{self, Collision};
use crate::profile::{self, Profiles};
use crate::{store_notice, term};

pub fn run(args: &GetArgs) -> Result<(), CliError> {
    let profiles = Profiles::load(&profile::config_dir()?)?;
    let profile = profiles.select(args.common.profile.as_deref())?;
    let out = match args
        .out
        .clone()
        .or_else(|| profile.and_then(|p| p.out.clone()))
    {
        Some(dir) => dir,
        None => std::env::current_dir()
            .map_err(|e| CliError::new(ExitClass::Output, format!("no current directory: {e}")))?,
    };

    let text = match &args.url {
        Some(url) => url.clone(),
        None => term::prompt_hidden("Link: ", "to ask for the link")?,
    };
    let link = ShareLink::parse(&text, args.common.insecure_http)?;
    drop(text);
    let api = Api::new(link.server.clone())?;
    let config = api.config()?;
    eprintln!("{}", store_notice(&config));
    eprintln!(
        "Opening deletes the message from the server, even if it then can't be decrypted. If the connection drops after that, the message is lost."
    );

    let (blob, pin) = loop {
        let pin = term::prompt_hidden("PIN: ", "for the PIN prompt")?;
        if !is_valid_pin(&pin) {
            eprintln!("The PIN must be exactly six digits.");
            continue;
        }
        match api.retrieve(&link.id, &pin)? {
            Retrieved::Blob(blob) => break (blob, pin),
            Retrieved::WrongPin => eprintln!("Wrong PIN. Too many wrong PINs delete the message."),
        }
    };

    let payload = open_payload(&blob, &link.passphrase, &pin).map_err(|error| {
        let why = if error.stage() == "decrypt" {
            "the link doesn't match it. Check that you copied the whole link"
        } else {
            "it is damaged or not a valid pinveil message"
        };
        CliError::new(
            ExitClass::Decryption,
            format!("cannot open this message: {why}. The server has already deleted it"),
        )
    })?;

    if let Some(text) = &payload.text {
        write_text(text, args.raw)?;
    }
    if !payload.files.is_empty() {
        let written = output::save(&payload.files, &out, args.force, ask_collision)?;
        for path in written {
            eprintln!("Saved {}", path.display());
        }
    }
    Ok(())
}

/// Text goes to stdout: unchanged into a pipe or file, neutralized on a terminal unless
/// `--raw`.
fn write_text(text: &str, raw: bool) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    let terminal = stdout.is_terminal();
    let result = if terminal && !raw {
        let shown = neutralize(text);
        stdout.write_all(shown.as_bytes()).and_then(|()| {
            if shown.ends_with('\n') {
                Ok(())
            } else {
                stdout.write_all(b"\n")
            }
        })
    } else {
        stdout.write_all(text.as_bytes())
    };
    result
        .and_then(|()| stdout.flush())
        .map_err(|e| CliError::new(ExitClass::Output, format!("cannot write the text: {e}")))
}

fn ask_collision(dir: &Path, taken: &[String]) -> Result<Collision, CliError> {
    let no_terminal = |_| {
        CliError::new(
            ExitClass::Output,
            format!(
                "{} file(s) already exist in {} and there is no terminal to ask what to do. Nothing was saved and the message is discarded; next time use --force or --out",
                taken.len(),
                dir.display()
            ),
        )
    };
    let question = format!(
        "These already exist in {}: {}\n[o]verwrite, save to [a]nother directory, or [q]uit (discards the message)? ",
        dir.display(),
        taken.join(", ")
    );
    loop {
        match term::ask(&question, "to ask about existing files")
            .map_err(no_terminal)?
            .to_ascii_lowercase()
            .as_str()
        {
            "o" | "overwrite" => return Ok(Collision::Overwrite),
            "q" | "quit" => return Ok(Collision::Quit),
            "a" | "another" => {
                let other =
                    term::ask("Directory: ", "to ask for a directory").map_err(no_terminal)?;
                if !other.is_empty() {
                    return Ok(Collision::Elsewhere(PathBuf::from(other)));
                }
            }
            _ => {}
        }
    }
}
