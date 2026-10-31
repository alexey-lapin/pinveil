//! Profiles: named, non-secret settings in a TOML file in the platform config directory
//! (APP.md, "Profiles"). A profile holds a server URL, a default TTL, and a default output
//! directory, nothing else. Unknown keys are refused so a secret can't be slipped in.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::args::ProfileArgs;
use crate::error::CliError;
use crate::links::ServerUrl;

const FILE_NAME: &str = "profiles.toml";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// The config directory: `PINVEIL_CONFIG_DIR` if set (for tests and portable setups),
/// otherwise the platform's, such as `~/.config/pinveil` on Linux.
pub fn config_dir() -> Result<PathBuf, CliError> {
    if let Some(dir) = std::env::var_os("PINVEIL_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "pinveil")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or_else(|| CliError::usage("cannot find a config directory for profiles"))
}

impl Profiles {
    /// Loads the profiles file and validates every profile in it, so an invalid one can't be
    /// used, shown, or saved again.
    pub fn load(dir: &Path) -> Result<Self, CliError> {
        let profiles = Self::load_unchecked(dir)?;
        profiles.validate_all(dir)?;
        Ok(profiles)
    }

    /// Loads without validating values, for `profile remove`, which must be able to delete
    /// a bad profile. Errors never quote the file, which may hold anything.
    fn load_unchecked(dir: &Path) -> Result<Self, CliError> {
        let path = dir.join(FILE_NAME);
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e: toml::de::Error| {
                let line = e
                    .span()
                    .map_or(0, |span| text[..span.start].matches('\n').count() + 1);
                CliError::usage(format!(
                    "{} is not a valid profiles file (problem at line {line}: a syntax error, an unknown key, or a wrong value type). Fix or remove it",
                    path.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CliError::usage(format!(
                "cannot read {}: {e}",
                path.display()
            ))),
        }
    }

    fn validate_all(&self, dir: &Path) -> Result<(), CliError> {
        for (name, profile) in &self.profiles {
            validate(profile).map_err(|e| {
                CliError::usage(format!(
                    "profile {name:?} in {}: {}. Fix it or run `pinveil profile remove {name}`",
                    dir.join(FILE_NAME).display(),
                    e.message
                ))
            })?;
        }
        if let Some(default) = &self.default
            && !self.profiles.contains_key(default)
        {
            return Err(CliError::usage(format!(
                "the default profile {default:?} in {} doesn't exist",
                dir.join(FILE_NAME).display()
            )));
        }
        Ok(())
    }

    /// Writes the file atomically, owner-only on Unix.
    pub fn save(&self, dir: &Path) -> Result<(), CliError> {
        let fail = |e: &dyn std::fmt::Display| {
            CliError::usage(format!("cannot save profiles in {}: {e}", dir.display()))
        };
        fs::create_dir_all(dir).map_err(|e| fail(&e))?;
        let text = toml::to_string_pretty(self).map_err(|e| fail(&e))?;
        let mut temp = tempfile::Builder::new()
            .prefix(".profiles-")
            .tempfile_in(dir)
            .map_err(|e| fail(&e))?;
        temp.write_all(text.as_bytes()).map_err(|e| fail(&e))?;
        temp.persist(dir.join(FILE_NAME))
            .map_err(|e| fail(&e.error))?;
        Ok(())
    }

    /// The profile to use: the named one (which must exist), else the default, if any.
    pub fn select(&self, name: Option<&str>) -> Result<Option<&Profile>, CliError> {
        match name {
            Some(name) => self
                .profiles
                .get(name)
                .map(Some)
                .ok_or_else(|| CliError::usage(format!("there is no profile named {name:?}"))),
            None => Ok(self
                .default
                .as_deref()
                .and_then(|name| self.profiles.get(name))),
        }
    }
}

/// `pinveil profile ...`.
pub fn run(args: ProfileArgs) -> Result<(), CliError> {
    let dir = config_dir()?;
    if let ProfileArgs::Remove(name) = &args {
        return remove(&dir, name);
    }
    let mut profiles = Profiles::load(&dir)?;
    match args {
        ProfileArgs::List => {
            for name in profiles.profiles.keys() {
                let mark = if profiles.default.as_deref() == Some(name) {
                    " (default)"
                } else {
                    ""
                };
                println!("{name}{mark}");
            }
            Ok(())
        }
        ProfileArgs::Show(name) => {
            let profile = profiles.select(Some(&name))?;
            let text = toml::to_string_pretty(&profile).expect("profiles serialize");
            print!("{text}");
            Ok(())
        }
        ProfileArgs::Set {
            name,
            server,
            ttl,
            out,
            default,
        } => {
            let profile = profiles.profiles.entry(name.clone()).or_default();
            if server.is_some() {
                profile.server = server;
            }
            if ttl.is_some() {
                profile.ttl = ttl;
            }
            if out.is_some() {
                profile.out = out;
            }
            validate(profile)?;
            if default {
                profiles.default = Some(name.clone());
            }
            profiles.save(&dir)?;
            eprintln!("Saved profile {name:?} in {}", dir.display());
            Ok(())
        }
        ProfileArgs::Remove(_) => unreachable!("handled above"),
    }
}

/// Removes a profile, even an invalid one. The rest must be valid to be saved again.
fn remove(dir: &Path, name: &str) -> Result<(), CliError> {
    let mut profiles = Profiles::load_unchecked(dir)?;
    if profiles.profiles.remove(name).is_none() {
        return Err(CliError::usage(format!(
            "there is no profile named {name:?}"
        )));
    }
    if profiles.default.as_deref() == Some(name) {
        profiles.default = None;
    }
    profiles.validate_all(dir)?;
    profiles.save(dir)?;
    eprintln!("Removed profile {name:?}");
    Ok(())
}

/// Checks a profile's values. Errors describe the problem without quoting the value.
pub fn validate(profile: &Profile) -> Result<(), CliError> {
    if let Some(server) = &profile.server {
        // Plain HTTP to a loopback IP may be stored; using it still needs that run's
        // --insecure-http. Here only the shape matters.
        ServerUrl::parse(server, true)
            .or_else(|_| ServerUrl::parse(server, false))
            .map_err(|e| CliError::usage(format!("invalid server URL: {e}")))?;
    }
    if let Some(ttl) = &profile.ttl {
        parse_ttl(ttl).map_err(|_| {
            CliError::usage("invalid TTL: use a whole number with s, m, h, or d, such as 15m")
        })?;
    }
    Ok(())
}

/// Parses a TTL such as `90s`, `15m`, `2h`, `1d`, or a plain number of seconds.
pub fn parse_ttl(text: &str) -> Result<u64, CliError> {
    let bad = || {
        CliError::usage(format!(
            "invalid TTL {text:?}: use a whole number with s, m, h, or d, such as 15m"
        ))
    };
    let (digits, unit) = match text.find(|c: char| !c.is_ascii_digit()) {
        Some(at) => text.split_at(at),
        None => (text, "s"),
    };
    let value: u64 = digits.parse().map_err(|_| bad())?;
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad()),
    };
    value.checked_mul(scale).filter(|&s| s > 0).ok_or_else(bad)
}

#[cfg(test)]
mod tests {
    use super::{Profile, Profiles, parse_ttl, validate};

    #[test]
    fn cli_profile_ttl_formats() {
        assert_eq!(parse_ttl("90").unwrap(), 90);
        assert_eq!(parse_ttl("90s").unwrap(), 90);
        assert_eq!(parse_ttl("15m").unwrap(), 900);
        assert_eq!(parse_ttl("2h").unwrap(), 7200);
        assert_eq!(parse_ttl("1d").unwrap(), 86_400);
        for bad in [
            "",
            "0",
            "0m",
            "1.5h",
            "15 m",
            "m",
            "1w",
            "-1m",
            "99999999999999999999d",
        ] {
            assert!(parse_ttl(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn cli_profile_round_trip_and_selection() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Profiles::load(dir.path()).unwrap(), Profiles::default());
        let mut profiles = Profiles::default();
        profiles.profiles.insert(
            "work".into(),
            Profile {
                server: Some("https://pinveil.example".into()),
                ttl: Some("1h".into()),
                out: None,
            },
        );
        profiles.default = Some("work".into());
        profiles.save(dir.path()).unwrap();
        let loaded = Profiles::load(dir.path()).unwrap();
        assert_eq!(loaded, profiles);
        assert_eq!(
            loaded.select(None).unwrap().unwrap().ttl.as_deref(),
            Some("1h")
        );
        assert!(loaded.select(Some("home")).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("profiles.toml"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn cli_profile_refuses_unknown_keys_and_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("profiles.toml"),
            "[profiles.x]\nserver = \"https://a\"\npin = \"123456\"\n",
        )
        .unwrap();
        assert!(Profiles::load(dir.path()).is_err());
        for profile in [
            Profile {
                server: Some("https://u:p@a".into()),
                ..Profile::default()
            },
            Profile {
                server: Some("https://a/?q".into()),
                ..Profile::default()
            },
            Profile {
                server: Some("https://a/#frag".into()),
                ..Profile::default()
            },
            Profile {
                ttl: Some("soon".into()),
                ..Profile::default()
            },
        ] {
            assert!(validate(&profile).is_err(), "{profile:?}");
        }
        assert!(
            validate(&Profile {
                server: Some("http://127.0.0.1:8080".into()),
                ..Profile::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn cli_profile_file_errors_never_quote_the_file() {
        let dir = tempfile::tempdir().unwrap();
        for text in [
            "[profiles.x]\npin = \"SENTINEL_SECRET\"\n",
            "[profiles.x]\nserver = \"https://e.test/message/id#SENTINEL_SECRET\" invalid\n",
            "[profiles.x]\nttl = [\"SENTINEL_SECRET\"]\n",
            "[profiles.x]\nserver = \"https://user:SENTINEL_SECRET@e.test\"\n",
            "[profiles.x]\nserver = \"https://e.test/message/id#SENTINEL_SECRET\"\n",
            "[profiles.x]\nserver = \"https://e.test/?q=SENTINEL_SECRET\"\n",
            "[profiles.x]\nttl = \"SENTINEL_SECRET\"\n",
        ] {
            std::fs::write(dir.path().join("profiles.toml"), text).unwrap();
            let error = Profiles::load(dir.path()).unwrap_err();
            assert!(
                !error.message.contains("SENTINEL"),
                "{text}: {}",
                error.message
            );
        }
    }

    #[test]
    fn cli_profile_invalid_loaded_profiles_are_never_saved_again() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("profiles.toml"),
            "[profiles.bad]\nserver = \"https://user:SECRET@e.test/message/id#FRAGMENT\"\n\n[profiles.good]\nttl = \"1h\"\n",
        )
        .unwrap();
        // Any command that loads the file refuses it, so `set` can't re-save the bad profile.
        assert!(Profiles::load(dir.path()).is_err());
        // `remove` can delete the bad one; the rest is saved.
        super::remove(dir.path(), "bad").unwrap();
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(
            !saved.contains("SECRET") && !saved.contains("FRAGMENT"),
            "{saved}"
        );
        assert!(
            Profiles::load(dir.path())
                .unwrap()
                .profiles
                .contains_key("good")
        );
    }
}
