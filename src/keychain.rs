//! Where the Pandora password lives: the desktop's keychain (the freedesktop
//! "Secret Service": GNOME Keyring, KeePassXC, KWallet's bridge and the like),
//! never `config.toml`.
//!
//! The app only knows the small [`Keychain`] trait, so the tests use
//! [`MemoryKeychain`] (no D-Bus on a build machine). [`system`] is the real
//! one: [`SecretServiceKeychain`], pure Rust (the `secret-service` crate over
//! `zbus`; no libdbus or OpenSSL to install).
//!
//! A keychain can be locked (the desktop may ask for its password) or not be
//! there at all, so the app never calls it on its main thread: see
//! `Backend::keychain`. When it fails, boxvibe keeps the password in memory for
//! the session and asks again next start.

use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;

/// The `service` attribute of the saved item.
#[cfg_attr(test, allow(dead_code))]
pub const SERVICE: &str = "boxvibe";

/// What the user is told when there is no keychain to keep the password in.
pub const NO_KEYCHAIN: &str = "No keychain found: you'll be asked for your password each start";

/// Why the keychain could not be used (shown only in the System Check hints
/// and kept for the log; the app says [`NO_KEYCHAIN`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct KeychainError(pub String);

/// Keeps one password per account (the Pandora email).
pub trait Keychain: Send + Sync {
    /// The saved password; `Ok(None)` when none is saved.
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError>;
    /// Saves (or replaces) the password.
    fn set(&self, account: &str, password: &str) -> Result<(), KeychainError>;
    /// Deletes the saved password. Fine when there is none.
    fn forget(&self, account: &str) -> Result<(), KeychainError>;
    /// Whether a keychain is there to use (for the System Check).
    fn available(&self) -> bool;
}

/// Passwords in memory only: the tests' keychain (and, made with `missing`,
/// the tests' "no keychain").
#[cfg(test)]
#[derive(Debug, Default)]
pub struct MemoryKeychain {
    items: Mutex<HashMap<String, String>>,
    /// `true`: every call fails like a machine with no Secret Service.
    broken: bool,
}

#[cfg(test)]
impl MemoryKeychain {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A keychain that isn't there.
    pub fn missing() -> Arc<Self> {
        Arc::new(Self { broken: true, ..Self::default() })
    }

    fn check(&self) -> Result<(), KeychainError> {
        if self.broken { Err(KeychainError("no Secret Service".into())) } else { Ok(()) }
    }

    /// What is saved, for assertions.
    pub fn saved(&self, account: &str) -> Option<String> {
        self.items.lock().ok()?.get(account).cloned()
    }
}

#[cfg(test)]
impl Keychain for MemoryKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        self.check()?;
        Ok(self.saved(account))
    }

    fn set(&self, account: &str, password: &str) -> Result<(), KeychainError> {
        self.check()?;
        if let Ok(mut items) = self.items.lock() {
            items.insert(account.to_owned(), password.to_owned());
        }
        Ok(())
    }

    fn forget(&self, account: &str) -> Result<(), KeychainError> {
        self.check()?;
        if let Ok(mut items) = self.items.lock() {
            items.remove(account);
        }
        Ok(())
    }

    fn available(&self) -> bool {
        !self.broken
    }
}

/// The desktop's Secret Service, over D-Bus.
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(test, allow(dead_code))]
pub struct SecretServiceKeychain;

#[cfg_attr(test, allow(dead_code))]
impl SecretServiceKeychain {
    /// Connects, unlocks the default collection (the desktop may ask), and
    /// hands both to `f`. Every item is found by its attributes, not its name.
    fn with<T>(
        f: impl for<'a> FnOnce(&'a secret_service::blocking::SecretService<'a>) -> Result<T, secret_service::Error>,
    ) -> Result<T, KeychainError> {
        use secret_service::EncryptionType;
        let service = secret_service::blocking::SecretService::connect(EncryptionType::Dh).map_err(describe)?;
        f(&service).map_err(describe)
    }

    fn attributes(account: &str) -> HashMap<&str, &str> {
        HashMap::from([("service", SERVICE), ("account", account)])
    }
}

#[cfg_attr(test, allow(dead_code))]
fn describe(e: secret_service::Error) -> KeychainError {
    KeychainError(e.to_string())
}

impl Keychain for SecretServiceKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        Self::with(|service| {
            let found = service.search_items(Self::attributes(account))?;
            // A locked item is unlocked (the desktop asks the user), then read.
            let item = match found.unlocked.first().or(found.locked.first()) {
                Some(item) => item,
                None => return Ok(None),
            };
            item.ensure_unlocked()?;
            let secret = item.get_secret()?;
            Ok(String::from_utf8(secret).ok().filter(|p| !p.is_empty()))
        })
    }

    fn set(&self, account: &str, password: &str) -> Result<(), KeychainError> {
        Self::with(|service| {
            let collection = service.get_any_collection()?;
            collection.ensure_unlocked()?;
            collection.create_item(
                &format!("boxvibe: Pandora password for {account}"),
                Self::attributes(account),
                password.as_bytes(),
                true,
                "text/plain",
            )?;
            Ok(())
        })
    }

    fn forget(&self, account: &str) -> Result<(), KeychainError> {
        Self::with(|service| {
            let found = service.search_items(Self::attributes(account))?;
            for item in found.unlocked.iter().chain(&found.locked) {
                item.ensure_unlocked()?;
                item.delete()?;
            }
            Ok(())
        })
    }

    fn available(&self) -> bool {
        Self::with(|service| service.get_any_collection().map(|_| ())).is_ok()
    }
}

/// The keychain the app uses: the real one, except in tests (they must not
/// touch the machine's keychain), where each app gets an empty memory one.
pub fn system() -> Arc<dyn Keychain> {
    #[cfg(test)]
    {
        MemoryKeychain::new()
    }
    #[cfg(not(test))]
    {
        Arc::new(SecretServiceKeychain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_keychain_keeps_one_password_per_account() {
        let k = MemoryKeychain::new();
        assert_eq!(k.get("a@x.com"), Ok(None));
        k.set("a@x.com", "one").unwrap();
        k.set("b@x.com", "two").unwrap();
        k.set("a@x.com", "three").unwrap();
        assert_eq!(k.get("a@x.com"), Ok(Some("three".into())), "replaced");
        assert_eq!(k.get("b@x.com"), Ok(Some("two".into())));
        k.forget("a@x.com").unwrap();
        assert_eq!(k.get("a@x.com"), Ok(None));
        k.forget("a@x.com").unwrap();
        assert!(k.available());
    }

    #[test]
    fn a_missing_keychain_fails_everything_and_says_it_is_not_there() {
        let k = MemoryKeychain::missing();
        assert!(!k.available());
        assert!(k.get("a").is_err() && k.set("a", "p").is_err() && k.forget("a").is_err());
    }
}
