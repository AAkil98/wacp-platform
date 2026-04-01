use std::collections::HashMap;

/// Manages secrets injected via configuration.
///
/// All values are wrapped in `SecretValue` which redacts itself in
/// Debug/Display output. Use `expose()` only when constructing auth headers.
pub struct SecretStore {
    secrets: HashMap<String, SecretValue>,
}

/// A secret value that redacts itself in Debug and Display.
pub struct SecretValue {
    value: String,
}

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Access the actual value. Use sparingly.
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretValue(***)")
    }
}

impl std::fmt::Display for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "***")
    }
}

impl SecretStore {
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }

    /// Store a secret. Overwrites if key exists.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.secrets
            .insert(key.into(), SecretValue::new(value));
    }

    /// Retrieve a secret. Returns None if not found.
    pub fn get(&self, key: &str) -> Option<&SecretValue> {
        self.secrets.get(key)
    }

    /// Check if a key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }

    /// Remove a secret. Returns true if it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        self.secrets.remove(key).is_some()
    }

    /// Number of stored secrets.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Scan a string for any stored secret values.
    /// Returns the key names of secrets found in the content.
    pub fn scan_for_leaks(&self, content: &str) -> Vec<String> {
        self.secrets
            .iter()
            .filter(|(_, v)| !v.value.is_empty() && content.contains(&v.value))
            .map(|(k, _)| k.clone())
            .collect()
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mut store = SecretStore::new();
        store.set("api_key", "sk-secret123");
        assert!(store.contains("api_key"));
        assert_eq!(store.get("api_key").unwrap().expose(), "sk-secret123");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = SecretStore::new();
        assert!(store.get("missing").is_none());
    }

    #[test]
    fn remove_existing() {
        let mut store = SecretStore::new();
        store.set("key", "value");
        assert!(store.remove("key"));
        assert!(!store.contains("key"));
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut store = SecretStore::new();
        assert!(!store.remove("missing"));
    }

    #[test]
    fn overwrite_existing_key() {
        let mut store = SecretStore::new();
        store.set("key", "old_value");
        store.set("key", "new_value");
        assert_eq!(store.get("key").unwrap().expose(), "new_value");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn len_and_is_empty() {
        let mut store = SecretStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.set("a", "1");
        store.set("b", "2");
        assert_eq!(store.len(), 2);
        assert!(!store.is_empty());
    }

    #[test]
    fn secret_value_debug_redacts() {
        let secret = SecretValue::new("super-secret-key");
        let debug = format!("{:?}", secret);
        assert_eq!(debug, "SecretValue(***)");
        assert!(!debug.contains("super-secret-key"));
    }

    #[test]
    fn secret_value_display_redacts() {
        let secret = SecretValue::new("super-secret-key");
        let display = format!("{}", secret);
        assert_eq!(display, "***");
    }

    #[test]
    fn expose_returns_actual_value() {
        let secret = SecretValue::new("the-real-value");
        assert_eq!(secret.expose(), "the-real-value");
    }

    #[test]
    fn scan_for_leaks_finds_secret() {
        let mut store = SecretStore::new();
        store.set("api_key", "sk-abc123secret");
        store.set("db_pass", "p@ssw0rd!");

        let leaks = store.scan_for_leaks("The API key is sk-abc123secret in this text");
        assert_eq!(leaks.len(), 1);
        assert!(leaks.contains(&"api_key".to_string()));
    }

    #[test]
    fn scan_for_leaks_clean_content() {
        let mut store = SecretStore::new();
        store.set("key", "secret-value");

        let leaks = store.scan_for_leaks("This text has no secrets in it");
        assert!(leaks.is_empty());
    }

    #[test]
    fn scan_for_leaks_multiple_matches() {
        let mut store = SecretStore::new();
        store.set("key1", "alpha");
        store.set("key2", "beta");

        let leaks = store.scan_for_leaks("Contains alpha and beta values");
        assert_eq!(leaks.len(), 2);
    }

    #[test]
    fn scan_for_leaks_skips_empty_values() {
        let mut store = SecretStore::new();
        store.set("empty", "");

        let leaks = store.scan_for_leaks("any content");
        assert!(leaks.is_empty()); // empty string not matched
    }
}
