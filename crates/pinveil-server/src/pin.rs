//! PIN verifiers (APP.md, "PIN verifier"). The server stores only
//! `Argon2id(pin + ":" + salt + ":" + pepper)`, as a PHC string that records its own
//! parameters, so changing the configured cost never breaks existing messages.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::config::PinConfig;

/// Hashes and checks PINs. Implementations are blocking and run on the hashing pool.
pub trait PinVerifier: Send + Sync + 'static {
    /// The stored verifier for `pin` under the base64 `salt`.
    fn hash(&self, pin: &str, salt: &str) -> String;
    /// Whether `pin` matches a verifier produced by [`PinVerifier::hash`].
    fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool;
}

/// Argon2id with the configured cost and the server pepper.
pub struct Argon2Verifier {
    argon2: Argon2<'static>,
    pepper: Zeroizing<String>,
}

impl Argon2Verifier {
    /// # Panics
    ///
    /// If the parameters are invalid; configuration validation rules that out at startup.
    #[must_use]
    pub fn new(config: &PinConfig) -> Self {
        let params = Params::new(
            config.argon2_memory_kib,
            config.argon2_iterations,
            config.argon2_parallelism,
            None,
        )
        .expect("validated Argon2 parameters");
        Self {
            argon2: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
            pepper: Zeroizing::new(config.pepper.clone()),
        }
    }

    fn input(&self, pin: &str, salt: &str) -> Zeroizing<String> {
        Zeroizing::new(format!("{pin}:{salt}:{}", self.pepper.as_str()))
    }
}

impl PinVerifier for Argon2Verifier {
    fn hash(&self, pin: &str, salt: &str) -> String {
        let input = self.input(pin, salt);
        self.argon2
            .hash_password_with_salt(input.as_bytes(), salt.as_bytes())
            .expect("Argon2id hashing with a 24-character salt")
            .to_string()
    }

    fn verify(&self, pin: &str, salt: &str, verifier: &str) -> bool {
        let Ok(parsed) = PasswordHash::new(verifier) else {
            return false;
        };
        let input = self.input(pin, salt);
        self.argon2
            .verify_password(input.as_bytes(), &parsed)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{Argon2Verifier, PinVerifier};
    use crate::config::{Sources, load};

    fn verifier(pepper: &str, memory_kib: &str) -> Argon2Verifier {
        let read = |_: &std::path::Path| Err(std::io::Error::other("none"));
        let (config, _) = load(&Sources {
            args: vec![],
            env: vec![
                ("PINVEIL_PIN__PEPPER".into(), pepper.into()),
                ("PINVEIL_PIN__ARGON2_MEMORY_KIB".into(), memory_kib.into()),
                ("PINVEIL_PIN__ARGON2_ITERATIONS".into(), "2".into()),
            ],
            read_file: &read,
            cpus: 1,
        })
        .unwrap();
        Argon2Verifier::new(&config.pin)
    }

    const SALT: &str = "AAECAwQFBgcICQoLDA0ODw==";

    #[test]
    fn inv_5_verifier_composition_and_params() {
        let v = verifier("pepper-one", "8192");
        let hash = v.hash("123456", SALT);
        assert!(hash.starts_with("$argon2id$v=19$m=8192,t=2,p=1$"), "{hash}");
        assert!(!hash.contains("123456") && !hash.contains("pepper-one"));
        assert!(v.verify("123456", SALT, &hash));
        assert!(!v.verify("123457", SALT, &hash), "wrong PIN");
        assert!(
            !v.verify("123456", "BBECAwQFBgcICQoLDA0ODw==", &hash),
            "wrong salt"
        );
        assert!(
            !verifier("pepper-two", "8192").verify("123456", SALT, &hash),
            "wrong pepper"
        );
        assert!(!v.verify("123456", SALT, "not a phc string"));
    }

    #[test]
    fn inv_5_params_applied_and_old_hashes_still_verify() {
        let old = verifier("p", "8192").hash("000000", SALT);
        let new = verifier("p", "16384");
        assert!(new.hash("000000", SALT).contains("m=16384"));
        assert!(
            new.verify("000000", SALT, &old),
            "the PHC string carries its own parameters"
        );
    }
}
