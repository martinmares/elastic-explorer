use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "elastic-explorer";

/// Uloží heslo do OS keychain
pub fn store_password(keychain_id: &str, password: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, keychain_id)
        .context("Failed to create keyring entry")?;

    entry.set_password(password)
        .context("Failed to store password in keychain")?;

    tracing::debug!("Password stored in keychain: {}", keychain_id);
    Ok(())
}

/// Načte heslo z OS keychain
pub fn get_password(keychain_id: &str) -> Result<String> {
    let entry = Entry::new(SERVICE_NAME, keychain_id)
        .context("Failed to create keyring entry")?;

    match entry.get_password() {
        Ok(password) => {
            tracing::debug!("Password retrieved from keychain: {}", keychain_id);
            Ok(password)
        }
        Err(e) => {
            tracing::error!("Keychain get_password error for {}: {:?}", keychain_id, e);
            Err(anyhow::anyhow!("Failed to retrieve password from keychain: {:?}", e))
        }
    }
}

/// Smaže heslo z OS keychain
pub fn delete_password(keychain_id: &str) -> Result<()> {
    let entry = Entry::new(SERVICE_NAME, keychain_id)
        .context("Failed to create keyring entry")?;

    entry.delete_credential()
        .context("Failed to delete password from keychain")?;

    tracing::debug!("Password deleted from keychain: {}", keychain_id);
    Ok(())
}

/// Vygeneruje keychain ID pro endpoint
pub fn generate_keychain_id(endpoint_id: i64) -> String {
    format!("endpoint-{}", endpoint_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Ignoruj v CI, protože vyžaduje OS keychain
    fn test_store_and_retrieve_password() {
        let test_id = "test-endpoint-999";
        let test_password = "super-secret-password";

        // Store
        store_password(test_id, test_password).unwrap();

        // Retrieve
        let retrieved = get_password(test_id).unwrap();
        assert_eq!(retrieved, test_password);

        // Cleanup
        delete_password(test_id).unwrap();
    }

    #[test]
    fn test_generate_keychain_id() {
        let id = generate_keychain_id(42);
        assert_eq!(id, "endpoint-42");
    }
}
