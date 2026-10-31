//! Server configuration (APP.md, "Configuration").
//!
//! Every setting is collected into one flat map of strings, layered in precedence order:
//! defaults, the TOML file, `PINVEIL_*` environment variables, then command-line flags.
//! Each value is then parsed with its own type and the whole configuration is validated.
//! Unknown keys are errors, so a typo fails at startup instead of being ignored.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ipnet::IpNet;
use pinveil_core::envelope::is_accepted_kdf_iterations;
use pinveil_core::protocol::{
    KDF_ITERATIONS_MAX, KDF_ITERATIONS_MIN, MAX_FILES_CEILING, MAX_MANIFEST_BYTES_CEILING,
};

/// Largest payload the browser's store-only ZIP32 writer can export.
const ZIP32_MAX_PAYLOAD_BYTES: usize = 4_000_000_000;

/// Defaults for every key. The key set is also the list of allowed keys.
const DEFAULTS: &[(&str, &str)] = &[
    ("server.listen", "0.0.0.0:8080"),
    ("server.trusted_proxies", ""),
    ("server.max_concurrent_uploads", "16"),
    ("messages.min_ttl", "1m"),
    ("messages.max_ttl", "24h"),
    ("messages.default_ttl", "15m"),
    ("messages.ttl_presets", "15m,30m,1h"),
    ("messages.max_payload_bytes", "26214400"),
    ("messages.max_files", "256"),
    ("messages.max_manifest_bytes", "262144"),
    ("messages.pbkdf2_iterations", "600000"),
    ("message_id.words", "3"),
    ("message_id.digits", "6"),
    ("store.mode", "memory"),
    ("store.path", ""),
    ("store.max_messages", "1000"),
    ("store.max_bytes", "268435456"),
    ("pin.pepper", ""),
    ("pin.max_failed_attempts", "3"),
    ("pin.argon2_iterations", "3"),
    ("pin.argon2_memory_kib", "19456"),
    ("pin.argon2_parallelism", "1"),
    ("pin.max_concurrent_hashes", ""),
    ("pin.max_hash_memory_bytes", "268435456"),
    ("pin.max_queued_hashes", "64"),
    ("rate_limit.window", "1m"),
    ("rate_limit.create_per_window", "10"),
    ("rate_limit.retrieve_per_window", "30"),
    ("rate_limit.max_tracked_keys", "100000"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub listen: SocketAddr,
    pub trusted_proxies: Vec<IpNet>,
    pub max_concurrent_uploads: usize,
    pub messages: MessagesConfig,
    pub message_id: MessageIdConfig,
    pub store: StoreConfig,
    pub pin: PinConfig,
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagesConfig {
    pub min_ttl: Duration,
    pub max_ttl: Duration,
    pub default_ttl: Duration,
    pub ttl_presets: Vec<Duration>,
    pub max_payload_bytes: usize,
    pub max_files: usize,
    pub max_manifest_bytes: usize,
    pub pbkdf2_iterations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageIdConfig {
    pub words: usize,
    pub digits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreMode {
    Memory,
    Sqlite,
}

impl StoreMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Sqlite => "sqlite",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    pub mode: StoreMode,
    pub path: Option<PathBuf>,
    pub max_messages: usize,
    pub max_bytes: usize,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PinConfig {
    pub pepper: String,
    pub max_failed_attempts: u32,
    pub argon2_iterations: u32,
    pub argon2_memory_kib: u32,
    pub argon2_parallelism: u32,
    pub max_concurrent_hashes: usize,
    pub max_hash_memory_bytes: usize,
    pub max_queued_hashes: usize,
}

// Hand-written so the pepper never reaches a log line.
impl std::fmt::Debug for PinConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinConfig")
            .field(
                "pepper",
                &if self.pepper.is_empty() {
                    "<empty>"
                } else {
                    "<set>"
                },
            )
            .field("max_failed_attempts", &self.max_failed_attempts)
            .field("argon2_iterations", &self.argon2_iterations)
            .field("argon2_memory_kib", &self.argon2_memory_kib)
            .field("argon2_parallelism", &self.argon2_parallelism)
            .field("max_concurrent_hashes", &self.max_concurrent_hashes)
            .field("max_hash_memory_bytes", &self.max_hash_memory_bytes)
            .field("max_queued_hashes", &self.max_queued_hashes)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitConfig {
    pub window: Duration,
    pub create_per_window: u32,
    pub retrieve_per_window: u32,
    pub max_tracked_keys: usize,
}

/// A configuration problem that stops the server from starting.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("configuration: {0}")]
pub struct ConfigError(pub String);

fn error(message: impl Into<String>) -> ConfigError {
    ConfigError(message.into())
}

/// Something worth logging at startup that doesn't stop the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(pub String);

/// Where configuration comes from. `read_file` is injectable for tests.
pub struct Sources<'a> {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub read_file: &'a dyn Fn(&Path) -> std::io::Result<String>,
    pub cpus: usize,
}

impl Sources<'_> {
    /// The real process arguments, environment, filesystem, and CPU count.
    pub fn from_process() -> Sources<'static> {
        Sources {
            args: std::env::args().skip(1).collect(),
            env: std::env::vars().collect(),
            read_file: &|path| std::fs::read_to_string(path),
            cpus: std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        }
    }
}

/// Loads and validates the configuration.
pub fn load(sources: &Sources<'_>) -> Result<(Config, Vec<Warning>), ConfigError> {
    let flags = parse_flags(&sources.args)?;
    let mut values: BTreeMap<String, String> = DEFAULTS
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();

    let config_path = flags
        .config
        .clone()
        .or_else(|| env_value(&sources.env, "PINVEIL_CONFIG").map(PathBuf::from));
    if let Some(path) = config_path {
        let text = (sources.read_file)(&path)
            .map_err(|e| error(format!("cannot read {}: {e}", path.display())))?;
        for (key, value) in flatten_toml(&text)? {
            set(&mut values, &key, value, "the config file")?;
        }
    }
    for (name, value) in &sources.env {
        // Only `PINVEIL_SECTION__KEY` names are settings, so other PINVEIL_ variables
        // (PINVEIL_CONFIG, test harness paths) are not mistaken for typos.
        if let Some(rest) = name
            .strip_prefix("PINVEIL_")
            .filter(|rest| rest.contains("__"))
        {
            let key = rest.to_lowercase().replace("__", ".");
            set(&mut values, &key, value.clone(), name)?;
        }
    }
    if let Some(listen) = flags.listen {
        set(&mut values, "server.listen", listen, "--listen")?;
    }
    build(&values, sources.cpus)
}

struct Flags {
    config: Option<PathBuf>,
    listen: Option<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, ConfigError> {
    let mut flags = Flags {
        config: None,
        listen: None,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let (name, slot): (&str, &mut Option<String>) = match arg.as_str() {
            "--listen" => ("--listen", &mut flags.listen),
            "--config" => {
                let value = iter.next().ok_or_else(|| error("--config needs a path"))?;
                if flags.config.replace(PathBuf::from(value)).is_some() {
                    return Err(error("--config was given twice"));
                }
                continue;
            }
            other => {
                return Err(error(format!(
                    "unknown argument {other:?}; usage: pinveil-server [--config PATH] [--listen ADDR]"
                )));
            }
        };
        let value = iter
            .next()
            .ok_or_else(|| error(format!("{name} needs a value")))?;
        if slot.replace(value.clone()).is_some() {
            return Err(error(format!("{name} was given twice")));
        }
    }
    Ok(flags)
}

fn env_value<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn set(
    values: &mut BTreeMap<String, String>,
    key: &str,
    value: String,
    source: &str,
) -> Result<(), ConfigError> {
    match values.get_mut(key) {
        Some(slot) => {
            *slot = value;
            Ok(())
        }
        None => Err(error(format!("unknown setting {key:?} (from {source})"))),
    }
}

/// Flattens `[section] key = value` into `section.key` strings. Lists become comma-separated.
fn flatten_toml(text: &str) -> Result<Vec<(String, String)>, ConfigError> {
    let table: toml::Table = text
        .parse()
        .map_err(|e| error(format!("invalid TOML: {e}")))?;
    let mut out = Vec::new();
    for (section, value) in table {
        let toml::Value::Table(entries) = value else {
            return Err(error(format!("{section:?} must be a [section]")));
        };
        for (key, value) in entries {
            let name = format!("{section}.{key}");
            out.push((name.clone(), scalar(&value, &name)?));
        }
    }
    Ok(out)
}

fn scalar(value: &toml::Value, name: &str) -> Result<String, ConfigError> {
    match value {
        toml::Value::String(s) => Ok(s.clone()),
        toml::Value::Integer(i) => Ok(i.to_string()),
        toml::Value::Array(items) => items
            .iter()
            .map(|item| match item {
                toml::Value::Array(_) | toml::Value::Table(_) => {
                    Err(error(format!("{name} must be a flat list")))
                }
                other => scalar(other, name),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join(",")),
        _ => Err(error(format!(
            "{name} must be a string, an integer, or a list"
        ))),
    }
}

struct Values<'a>(&'a BTreeMap<String, String>);

impl Values<'_> {
    fn raw(&self, key: &str) -> &str {
        self.0.get(key).map_or("", String::as_str)
    }

    fn parse<T: std::str::FromStr>(&self, key: &str) -> Result<T, ConfigError> {
        self.raw(key)
            .trim()
            .parse()
            .map_err(|_| error(format!("{key} = {:?} is not valid", self.raw(key))))
    }

    fn duration(&self, key: &str) -> Result<Duration, ConfigError> {
        parse_duration(self.raw(key)).map_err(|()| {
            error(format!(
                "{key} = {:?} is not a duration such as 15m",
                self.raw(key)
            ))
        })
    }

    fn list(&self, key: &str) -> Vec<&str> {
        self.raw(key)
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    }
}

fn parse_duration(text: &str) -> Result<Duration, ()> {
    humantime::parse_duration(text.trim()).map_err(|_| ())
}

fn build(
    values: &BTreeMap<String, String>,
    cpus: usize,
) -> Result<(Config, Vec<Warning>), ConfigError> {
    let v = Values(values);
    let mut warnings = Vec::new();
    let messages = build_messages(&v)?;
    let message_id = MessageIdConfig {
        words: v.parse("message_id.words")?,
        digits: v.parse("message_id.digits")?,
    };
    if !(1..=8).contains(&message_id.words) || !(1..=9).contains(&message_id.digits) {
        return Err(error(
            "message_id.words must be 1 to 8 and message_id.digits 1 to 9",
        ));
    }
    let store = build_store(&v, &messages)?;
    let pin = build_pin(&v, cpus, &mut warnings)?;
    let rate_limit = build_rate_limit(&v)?;
    let config = Config {
        listen: v.parse("server.listen")?,
        trusted_proxies: v
            .list("server.trusted_proxies")
            .into_iter()
            .map(|p| {
                parse_net(p).ok_or_else(|| {
                    error(format!(
                        "server.trusted_proxies has an invalid address {p:?}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?,
        max_concurrent_uploads: v.parse("server.max_concurrent_uploads")?,
        messages,
        message_id,
        store,
        pin,
        rate_limit,
    };
    if config.max_concurrent_uploads == 0 {
        return Err(error("server.max_concurrent_uploads must be positive"));
    }
    Ok((config, warnings))
}

fn build_messages(v: &Values<'_>) -> Result<MessagesConfig, ConfigError> {
    let messages = MessagesConfig {
        min_ttl: v.duration("messages.min_ttl")?,
        max_ttl: v.duration("messages.max_ttl")?,
        default_ttl: v.duration("messages.default_ttl")?,
        ttl_presets: v
            .list("messages.ttl_presets")
            .into_iter()
            .map(|p| {
                parse_duration(p).map_err(|()| {
                    error(format!(
                        "messages.ttl_presets has an invalid duration {p:?}"
                    ))
                })
            })
            .collect::<Result<_, _>>()?,
        max_payload_bytes: v.parse("messages.max_payload_bytes")?,
        max_files: v.parse("messages.max_files")?,
        max_manifest_bytes: v.parse("messages.max_manifest_bytes")?,
        pbkdf2_iterations: v.parse("messages.pbkdf2_iterations")?,
    };
    if messages.min_ttl < Duration::from_secs(1) || messages.min_ttl > messages.max_ttl {
        return Err(error(
            "messages.min_ttl must be at least 1s and at most messages.max_ttl",
        ));
    }
    let in_range = |d: &Duration| (messages.min_ttl..=messages.max_ttl).contains(d);
    if !in_range(&messages.default_ttl) || !messages.ttl_presets.iter().all(in_range) {
        return Err(error(
            "messages.default_ttl and messages.ttl_presets must lie between min_ttl and max_ttl",
        ));
    }
    if messages.max_payload_bytes == 0 || messages.max_payload_bytes > ZIP32_MAX_PAYLOAD_BYTES {
        return Err(error(format!(
            "messages.max_payload_bytes must be between 1 and {ZIP32_MAX_PAYLOAD_BYTES} (the browser zip export uses ZIP32)"
        )));
    }
    if !(1..=MAX_FILES_CEILING).contains(&messages.max_files) {
        return Err(error(format!(
            "messages.max_files must be between 1 and the protocol ceiling {MAX_FILES_CEILING}"
        )));
    }
    if !(1..=MAX_MANIFEST_BYTES_CEILING).contains(&messages.max_manifest_bytes) {
        return Err(error(format!(
            "messages.max_manifest_bytes must be between 1 and the protocol ceiling {MAX_MANIFEST_BYTES_CEILING}"
        )));
    }
    if !is_accepted_kdf_iterations(messages.pbkdf2_iterations) {
        return Err(error(format!(
            "messages.pbkdf2_iterations must be between {KDF_ITERATIONS_MIN} and {KDF_ITERATIONS_MAX}"
        )));
    }
    Ok(messages)
}

fn build_store(v: &Values<'_>, messages: &MessagesConfig) -> Result<StoreConfig, ConfigError> {
    let store = StoreConfig {
        mode: match v.raw("store.mode").trim() {
            "memory" => StoreMode::Memory,
            "sqlite" => StoreMode::Sqlite,
            other => {
                return Err(error(format!(
                    "store.mode must be memory or sqlite, not {other:?}"
                )));
            }
        },
        path: Some(v.raw("store.path").trim())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from),
        max_messages: v.parse("store.max_messages")?,
        max_bytes: v.parse("store.max_bytes")?,
    };
    if store.mode == StoreMode::Sqlite {
        if store.path.is_none() {
            return Err(error("store.path is required when store.mode = sqlite"));
        }
        return Err(error(
            "store.mode = sqlite is not available in this build yet",
        ));
    }
    if store.max_messages == 0 || store.max_bytes < messages.max_payload_bytes {
        return Err(error(
            "store.max_messages must be positive and store.max_bytes at least messages.max_payload_bytes",
        ));
    }
    Ok(store)
}

fn build_pin(
    v: &Values<'_>,
    cpus: usize,
    warnings: &mut Vec<Warning>,
) -> Result<PinConfig, ConfigError> {
    let argon2_memory_kib: u32 = v.parse("pin.argon2_memory_kib")?;
    let max_hash_memory_bytes: usize = v.parse("pin.max_hash_memory_bytes")?;
    let explicit: Option<usize> = match v.raw("pin.max_concurrent_hashes").trim() {
        "" => None,
        _ => Some(v.parse("pin.max_concurrent_hashes")?),
    };
    let (max_concurrent_hashes, capped) =
        hash_concurrency(cpus, max_hash_memory_bytes, argon2_memory_kib, explicit)?;
    if capped {
        warnings.push(Warning(format!(
            "pin.max_concurrent_hashes lowered to {max_concurrent_hashes} to fit pin.max_hash_memory_bytes"
        )));
    }
    let pin = PinConfig {
        pepper: v.raw("pin.pepper").to_owned(),
        max_failed_attempts: v.parse("pin.max_failed_attempts")?,
        argon2_iterations: v.parse("pin.argon2_iterations")?,
        argon2_memory_kib,
        argon2_parallelism: v.parse("pin.argon2_parallelism")?,
        max_concurrent_hashes,
        max_hash_memory_bytes,
        max_queued_hashes: v.parse("pin.max_queued_hashes")?,
    };
    if pin.max_failed_attempts == 0 || pin.max_queued_hashes == 0 {
        return Err(error(
            "pin.max_failed_attempts and pin.max_queued_hashes must be positive",
        ));
    }
    if argon2::Params::new(
        pin.argon2_memory_kib,
        pin.argon2_iterations,
        pin.argon2_parallelism,
        None,
    )
    .is_err()
    {
        return Err(error(
            "pin.argon2_* parameters are not valid Argon2id parameters",
        ));
    }
    if pin.pepper.is_empty() {
        warnings.push(Warning(
            "pin.pepper is empty; set a secret pepper and keep it stable".into(),
        ));
    }
    Ok(pin)
}

fn build_rate_limit(v: &Values<'_>) -> Result<RateLimitConfig, ConfigError> {
    let rate_limit = RateLimitConfig {
        window: v.duration("rate_limit.window")?,
        create_per_window: v.parse("rate_limit.create_per_window")?,
        retrieve_per_window: v.parse("rate_limit.retrieve_per_window")?,
        max_tracked_keys: v.parse("rate_limit.max_tracked_keys")?,
    };
    if rate_limit.window < Duration::from_secs(1)
        || rate_limit.create_per_window == 0
        || rate_limit.retrieve_per_window == 0
        || rate_limit.max_tracked_keys == 0
    {
        return Err(error(
            "rate_limit.window must be at least 1s and the other rate limits positive",
        ));
    }
    Ok(rate_limit)
}

/// Accepts a CIDR (`10.0.0.0/8`) or a bare address (`127.0.0.1`, a /32 or /128).
fn parse_net(text: &str) -> Option<IpNet> {
    text.parse::<IpNet>()
        .ok()
        .or_else(|| text.parse::<std::net::IpAddr>().ok().map(IpNet::from))
}

/// `min(CPU count, floor(budget / (memory_kib * 1024)))`, with an explicit value capped by
/// the same budget. Returns whether an explicit value was lowered.
fn hash_concurrency(
    cpus: usize,
    budget_bytes: usize,
    memory_kib: u32,
    explicit: Option<usize>,
) -> Result<(usize, bool), ConfigError> {
    let per_hash = memory_kib as usize * 1024;
    let fits = budget_bytes / per_hash.max(1);
    if fits == 0 {
        return Err(error(
            "pin.max_hash_memory_bytes is smaller than one Argon2 hash (pin.argon2_memory_kib * 1024)",
        ));
    }
    match explicit {
        Some(0) => Err(error("pin.max_concurrent_hashes must be positive")),
        Some(n) if n > fits => Ok((fits, true)),
        Some(n) => Ok((n, false)),
        None => Ok((cpus.max(1).min(fits), false)),
    }
}

/// Human-readable TTL preset label: whole hours, whole minutes, or seconds.
#[must_use]
pub fn ttl_label(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    if seconds.is_multiple_of(3600) {
        plural(seconds / 3600, "hour")
    } else if seconds.is_multiple_of(60) {
        plural(seconds / 60, "minute")
    } else {
        plural(seconds, "second")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::{ConfigError, Sources, Warning, hash_concurrency, load, ttl_label};

    fn load_with(
        args: &[&str],
        env: &[(&str, &str)],
        file: Option<&str>,
    ) -> Result<(super::Config, Vec<Warning>), ConfigError> {
        let file = file.map(str::to_owned);
        let read = move |_: &Path| file.clone().ok_or_else(|| std::io::Error::other("no file"));
        load(&Sources {
            args: args.iter().map(|s| (*s).to_owned()).collect(),
            env: env
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            read_file: &read,
            cpus: 32,
        })
    }

    fn err(result: Result<(super::Config, Vec<Warning>), ConfigError>) -> String {
        result.unwrap_err().0
    }

    #[test]
    fn cfg_defaults_match_app_md() {
        let (config, warnings) = load_with(&[], &[], None).unwrap();
        assert_eq!(config.listen.port(), 8080);
        assert_eq!(
            config.messages.ttl_presets,
            [15 * 60, 30 * 60, 3600].map(Duration::from_secs)
        );
        assert_eq!(config.messages.max_payload_bytes, 26_214_400);
        assert_eq!(
            config.pin.max_concurrent_hashes, 13,
            "256 MiB / 19 MiB = 13, below 32 CPUs"
        );
        assert_eq!(config.rate_limit.create_per_window, 10);
        assert!(config.trusted_proxies.is_empty());
        assert_eq!(
            warnings,
            [Warning(
                "pin.pepper is empty; set a secret pepper and keep it stable".into()
            )],
            "CFG-pepper-warn"
        );
    }

    #[test]
    fn cfg_precedence_flags_env_file_defaults() {
        let file = "[server]\nlisten = \"127.0.0.1:1000\"\n[pin]\npepper = \"from-file\"\nmax_failed_attempts = 5\n";
        let (config, _) = load_with(
            &["--config", "x.toml"],
            &[("PINVEIL_PIN__PEPPER", "from-env")],
            Some(file),
        )
        .unwrap();
        assert_eq!(config.listen.port(), 1000, "file beats default");
        assert_eq!(config.pin.pepper, "from-env", "env beats file");
        assert_eq!(config.pin.max_failed_attempts, 5);
        let (config, _) = load_with(
            &["--config", "x.toml", "--listen", "127.0.0.1:2000"],
            &[("PINVEIL_SERVER__LISTEN", "127.0.0.1:3000")],
            Some(file),
        )
        .unwrap();
        assert_eq!(config.listen.port(), 2000, "flag beats env and file");
    }

    #[test]
    fn cfg_precedence_config_flag_beats_env_path() {
        let read = |path: &Path| {
            Ok(if path == Path::new("flag.toml") {
                "[pin]\nmax_failed_attempts = 7\n"
            } else {
                "[pin]\nmax_failed_attempts = 9\n"
            }
            .to_owned())
        };
        let sources = |args: Vec<String>| Sources {
            args,
            env: vec![("PINVEIL_CONFIG".into(), "env.toml".into())],
            read_file: &read,
            cpus: 4,
        };
        assert_eq!(load(&sources(vec![])).unwrap().0.pin.max_failed_attempts, 9);
        assert_eq!(
            load(&sources(vec!["--config".into(), "flag.toml".into()]))
                .unwrap()
                .0
                .pin
                .max_failed_attempts,
            7
        );
    }

    #[test]
    fn cfg_precedence_lists_and_toml_arrays() {
        let (config, _) = load_with(
            &[],
            &[
                ("PINVEIL_MESSAGES__TTL_PRESETS", "5m, 1h"),
                ("PINVEIL_SERVER__TRUSTED_PROXIES", "10.0.0.0/8,::1"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            config.messages.ttl_presets,
            [300, 3600].map(Duration::from_secs)
        );
        assert_eq!(config.trusted_proxies.len(), 2);
        let (config, _) = load_with(
            &["--config", "c"],
            &[],
            Some("[messages]\nttl_presets = [\"10m\", \"2h\"]\n"),
        )
        .unwrap();
        assert_eq!(
            config.messages.ttl_presets,
            [600, 7200].map(Duration::from_secs)
        );
    }

    #[test]
    fn cfg_precedence_repeated_and_unknown_options_are_errors() {
        assert!(
            err(load_with(
                &["--listen", "127.0.0.1:1", "--listen", "127.0.0.1:2"],
                &[],
                None
            ))
            .contains("twice")
        );
        assert!(err(load_with(&["--config", "a", "--config", "b"], &[], None)).contains("twice"));
        assert!(err(load_with(&["--verbose"], &[], None)).contains("unknown argument"));
        assert!(err(load_with(&[], &[("PINVEIL_PIN__PEPER", "x")], None)).contains("pin.peper"));
        assert!(
            err(load_with(
                &["--config", "c"],
                &[],
                Some("[pin]\npeper = \"x\"\n")
            ))
            .contains("pin.peper")
        );
        // PINVEIL_ variables without a section separator are not settings.
        assert!(
            load_with(
                &[],
                &[("PINVEIL_SERVER_BIN", "/x"), ("PINVEIL_CONFIG_UNUSED", "y")],
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn cfg_validate_refuses_impossible_settings() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "PINVEIL_MESSAGES__DEFAULT_TTL",
                "48h",
                "between min_ttl and max_ttl",
            ),
            (
                "PINVEIL_MESSAGES__TTL_PRESETS",
                "15m,2d",
                "between min_ttl and max_ttl",
            ),
            ("PINVEIL_MESSAGES__MIN_TTL", "0s", "at least 1s"),
            (
                "PINVEIL_MESSAGES__PBKDF2_ITERATIONS",
                "100000",
                "pbkdf2_iterations",
            ),
            (
                "PINVEIL_MESSAGES__PBKDF2_ITERATIONS",
                "3000000",
                "pbkdf2_iterations",
            ),
            ("PINVEIL_MESSAGES__MAX_FILES", "257", "protocol ceiling"),
            (
                "PINVEIL_MESSAGES__MAX_MANIFEST_BYTES",
                "300000",
                "protocol ceiling",
            ),
            ("PINVEIL_MESSAGES__MAX_PAYLOAD_BYTES", "5000000000", "ZIP32"),
            ("PINVEIL_STORE__MODE", "redis", "memory or sqlite"),
            ("PINVEIL_STORE__MODE", "sqlite", "store.path is required"),
            ("PINVEIL_STORE__MAX_BYTES", "1000", "store.max_bytes"),
            (
                "PINVEIL_PIN__MAX_HASH_MEMORY_BYTES",
                "1000",
                "smaller than one Argon2 hash",
            ),
            (
                "PINVEIL_PIN__MAX_CONCURRENT_HASHES",
                "0",
                "must be positive",
            ),
            (
                "PINVEIL_SERVER__TRUSTED_PROXIES",
                "not-an-ip",
                "trusted_proxies",
            ),
            ("PINVEIL_RATE_LIMIT__WINDOW", "soon", "not a duration"),
            ("PINVEIL_MESSAGE_ID__DIGITS", "12", "message_id"),
        ];
        for (key, value, expected) in cases {
            let message = err(load_with(&[], &[(key, value)], None));
            assert!(message.contains(expected), "{key}={value}: {message}");
        }
    }

    #[test]
    fn cfg_hash_concurrency_formula() {
        let mib = 1024 * 1024;
        assert_eq!(
            hash_concurrency(32, 256 * mib, 19_456, None).unwrap(),
            (13, false)
        );
        assert_eq!(
            hash_concurrency(4, 256 * mib, 19_456, None).unwrap(),
            (4, false)
        );
        assert_eq!(
            hash_concurrency(4, 256 * mib, 19_456, Some(20)).unwrap(),
            (13, true),
            "explicit values are capped"
        );
        assert_eq!(
            hash_concurrency(4, 256 * mib, 19_456, Some(2)).unwrap(),
            (2, false)
        );
        assert!(hash_concurrency(4, 19_456 * 1024 - 1, 19_456, None).is_err());
        let (config, warnings) = load_with(
            &[],
            &[
                ("PINVEIL_PIN__MAX_CONCURRENT_HASHES", "50"),
                ("PINVEIL_PIN__PEPPER", "p"),
            ],
            None,
        )
        .unwrap();
        assert_eq!(config.pin.max_concurrent_hashes, 13);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn pepper_never_appears_in_debug_output() {
        let (config, _) =
            load_with(&[], &[("PINVEIL_PIN__PEPPER", "super-secret-pepper")], None).unwrap();
        assert!(!format!("{config:?}").contains("super-secret-pepper"));
    }

    #[test]
    fn ttl_labels() {
        assert_eq!(ttl_label(Duration::from_mins(15)), "15 minutes");
        assert_eq!(ttl_label(Duration::from_hours(1)), "1 hour");
        assert_eq!(ttl_label(Duration::from_hours(2)), "2 hours");
        assert_eq!(ttl_label(Duration::from_mins(1)), "1 minute");
        assert_eq!(ttl_label(Duration::from_secs(90)), "90 seconds");
    }
}
