use crate::api_client::ApiError;
use crate::config;

const SERVICE_NAME: &str = "meticulous-cli";
const TOKEN_KEY: &str = "auth_token";

#[cfg(feature = "keyring")]
pub fn store_token(token: &str) -> Result<(), ApiError> {
    let entry = keyring::Entry::new(SERVICE_NAME, TOKEN_KEY)
        .map_err(|e| ApiError::Other(format!("Keyring error: {}", e)))?;
    entry
        .set_password(token)
        .map_err(|e| ApiError::Other(format!("Failed to store token in keyring: {}", e)))?;
    Ok(())
}

#[cfg(feature = "keyring")]
pub fn load_token() -> Option<String> {
    let entry = keyring::Entry::new(SERVICE_NAME, TOKEN_KEY).ok()?;
    entry.get_password().ok()
}

#[cfg(feature = "keyring")]
pub fn clear_token() -> Result<(), ApiError> {
    let entry = keyring::Entry::new(SERVICE_NAME, TOKEN_KEY)
        .map_err(|e| ApiError::Other(format!("Keyring error: {}", e)))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(ApiError::Other(format!(
            "Failed to clear token from keyring: {}",
            e
        ))),
    }
}

#[cfg(not(feature = "keyring"))]
pub fn store_token(token: &str) -> Result<(), ApiError> {
    let path = token_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Other(format!("Failed to create config dir: {}", e)))?;
    }
    std::fs::write(&path, token)
        .map_err(|e| ApiError::Other(format!("Failed to write token file: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .map_err(|e| ApiError::Other(format!("Failed to set file permissions: {}", e)))?;
    }

    Ok(())
}

#[cfg(not(feature = "keyring"))]
pub fn load_token() -> Option<String> {
    let path = token_file_path().ok()?;
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

#[cfg(not(feature = "keyring"))]
pub fn clear_token() -> Result<(), ApiError> {
    let path = token_file_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ApiError::Other(format!(
            "Failed to remove token file: {}",
            e
        ))),
    }
}

#[cfg(not(feature = "keyring"))]
fn token_file_path() -> Result<std::path::PathBuf, ApiError> {
    config::global_config_dir()
        .map(|d| d.join("token"))
        .ok_or_else(|| ApiError::Other("Could not determine config directory".into()))
}
