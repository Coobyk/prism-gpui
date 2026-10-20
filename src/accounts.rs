//! Account store. Mirrors launcher/minecraft/auth/AccountList (`accounts.json`).

use crate::auth::Account;
use std::path::{Path, PathBuf};

pub const STORE_FILE: &str = "accounts.json";

#[derive(Debug, Clone, Default)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    pub active: Option<String>,
}

impl AccountStore {
    pub fn load(data_root: &Path) -> Self {
        let path = data_root.join(STORE_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        let accounts = value
            .get("accounts")
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        Some(Account {
                            username: a.get("username")?.as_str()?.to_string(),
                            uuid: a.get("uuid")?.as_str()?.to_string(),
                            access_token: a
                                .get("access_token")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            refresh_token: a
                                .get("refresh_token")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            expires_at: a.get("expires_at").and_then(|v| v.as_u64()).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = value
            .get("active")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Self { accounts, active }
    }

    pub fn save(&self, data_root: &Path) -> Result<(), String> {
        let path: PathBuf = data_root.join(STORE_FILE);
        let accounts: Vec<serde_json::Value> = self
            .accounts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "username": a.username,
                    "uuid": a.uuid,
                    "access_token": a.access_token,
                    "refresh_token": a.refresh_token,
                    "expires_at": a.expires_at,
                })
            })
            .collect();
        let value = serde_json::json!({
            "accounts": accounts,
            "active": self.active,
        });
        let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    }

    /// Add or replace an account (keyed by uuid); returns its index.
    pub fn upsert(&mut self, account: Account) -> usize {
        if let Some(i) = self.accounts.iter().position(|a| a.uuid == account.uuid) {
            self.accounts[i] = account.clone();
            self.active = Some(account.uuid);
            i
        } else {
            self.accounts.push(account.clone());
            self.active = Some(account.uuid);
            self.accounts.len() - 1
        }
    }

    pub fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active.as_deref() == Some(uuid) {
            self.active = self.accounts.first().map(|a| a.uuid.clone());
        }
    }

    pub fn active_account(&self) -> Option<&Account> {
        let active = self.active.as_deref()?;
        self.accounts.iter().find(|a| a.uuid == active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Account;

    fn acct(name: &str, uuid: &str) -> Account {
        Account {
            username: name.into(),
            uuid: uuid.into(),
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            expires_at: 123,
        }
    }

    #[test]
    fn upsert_remove_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let mut store = AccountStore::default();
        store.upsert(acct("Steve", "u1"));
        store.upsert(acct("Alex", "u2"));
        store.save(td.path()).unwrap();

        let mut loaded = AccountStore::load(td.path());
        assert_eq!(loaded.accounts.len(), 2);
        assert_eq!(loaded.active_account().unwrap().username, "Alex");

        // Re-upsert replaces in place.
        loaded.upsert(acct("Steve2", "u1"));
        assert_eq!(loaded.accounts.len(), 2);

        loaded.remove("u1");
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.active_account().unwrap().uuid, "u2");
    }
}

/// Derive the offline-mode UUID for a username (MD5 of "OfflinePlayer:<name>",
/// with UUID v3 version/variant bits), matching vanilla behaviour.
pub fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = hasher.finalize();
    bytes[6] = (bytes[6] & 0x0f) | 0x30; // version 3
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Build an offline (unauthenticated) account for local/testing use.
pub fn offline_account(name: &str) -> Account {
    Account {
        username: name.to_string(),
        uuid: offline_uuid(name),
        access_token: "offline".to_string(),
        refresh_token: String::new(),
        expires_at: 0,
    }
}

#[cfg(test)]
mod offline_tests {
    use super::*;
    #[test]
    fn offline_uuid_is_stable_and_versioned() {
        let u = offline_uuid("Player");
        assert_eq!(u.len(), 36);
        // UUID v3 -> third group starts with '3'.
        let third = u.split('-').nth(2).unwrap();
        assert!(third.starts_with('3'));
        assert_eq!(u, offline_uuid("Player"));
        assert_ne!(u, offline_uuid("Someone"));
    }
}

/// Pick the account an instance should launch with.
///
/// An instance may pin a specific account (`UseAccountForInstance` +
/// `InstanceAccountId`); otherwise the globally active account is used.
pub fn account_for_instance(
    data_root: &Path,
    inst_dir: &Path,
) -> Option<crate::auth::Account> {
    let store = AccountStore::load(data_root);
    if let Some(inst) = crate::instance::Instance::load(inst_dir) {
        let pinned = inst
            .settings
            .get("UseAccountForInstance")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        if pinned {
            let uuid = inst.settings.get("InstanceAccountId").cloned().unwrap_or_default();
            if let Some(account) = store.accounts.iter().find(|a| a.uuid == uuid) {
                return Some(account.clone());
            }
        }
    }
    store.active_account().cloned()
}

#[cfg(test)]
mod instance_account_tests {
    use super::*;

    #[test]
    fn falls_back_to_the_active_account() {
        let td = tempfile::tempdir().unwrap();
        let inst_dir = td.path().join("inst");
        std::fs::create_dir_all(&inst_dir).unwrap();
        std::fs::write(inst_dir.join("instance.cfg"), "[General]\n").unwrap();
        assert!(account_for_instance(td.path(), &inst_dir).is_none());
    }
}
