use lecturescribe_core::{AppError, ErrorCategory};
use sha2::{Digest, Sha256};

const SERVICE: &str = "LectureScribe";
const GEMINI_ACCOUNT: &str = "gemini_api_key";
const GEMINI_VERIFICATION_ACCOUNT: &str = "gemini_api_key_verification";

#[derive(Debug, Clone, Default)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn save_gemini_key(&self, value: &str) -> Result<(), AppError> {
        let value = value.trim();
        if !valid_api_key(value) {
            return Err(AppError::new(
                "api_key_invalid_format",
                ErrorCategory::Authentication,
                "Enter a valid Gemini API key.",
                "The supplied credential was empty or matched the placeholder value.",
            ));
        }
        self.clear_gemini_verification()?;
        keyring::Entry::new(SERVICE, GEMINI_ACCOUNT)
            .map_err(keyring_error)?
            .set_password(value)
            .map_err(keyring_error)
    }

    pub fn gemini_key(&self) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new(SERVICE, GEMINI_ACCOUNT).map_err(keyring_error)?;
        match entry.get_password() {
            Ok(value) if valid_api_key(&value) => Ok(Some(value)),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keyring_error(error)),
        }
    }

    pub fn delete_gemini_key(&self) -> Result<(), AppError> {
        let entry = keyring::Entry::new(SERVICE, GEMINI_ACCOUNT).map_err(keyring_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error(error)),
        }?;
        self.clear_gemini_verification()
    }

    pub fn configured(&self) -> bool {
        self.gemini_key().ok().flatten().is_some()
    }

    pub fn mark_gemini_key_verified(&self) -> Result<(), AppError> {
        let key = self.gemini_key()?.ok_or_else(|| {
            AppError::new(
                "api_key_missing",
                ErrorCategory::Authentication,
                "Add a Gemini API key before verifying it.",
                "No Gemini credential was present in Windows Credential Manager.",
            )
        })?;
        keyring::Entry::new(SERVICE, GEMINI_VERIFICATION_ACCOUNT)
            .map_err(keyring_error)?
            .set_password(&key_fingerprint(&key))
            .map_err(keyring_error)
    }

    pub fn gemini_key_verified(&self) -> bool {
        let Ok(Some(key)) = self.gemini_key() else {
            return false;
        };
        let Ok(entry) = keyring::Entry::new(SERVICE, GEMINI_VERIFICATION_ACCOUNT) else {
            return false;
        };
        entry
            .get_password()
            .is_ok_and(|value| value == key_fingerprint(&key))
    }

    fn clear_gemini_verification(&self) -> Result<(), AppError> {
        let entry =
            keyring::Entry::new(SERVICE, GEMINI_VERIFICATION_ACCOUNT).map_err(keyring_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error(error)),
        }
    }
}

fn key_fingerprint(value: &str) -> String {
    hex::encode(Sha256::digest(value.trim().as_bytes()))
}

fn valid_api_key(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 20 && value != "put-your-gemini-api-key-here"
}

fn keyring_error(error: keyring::Error) -> AppError {
    AppError::new(
        "credential_store_failed",
        ErrorCategory::Authentication,
        "LectureScribe could not access Windows Credential Manager.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_fingerprint_is_stable_and_ignores_outer_whitespace() {
        assert_eq!(key_fingerprint("test-key"), key_fingerprint(" test-key "));
        assert_ne!(key_fingerprint("test-key"), key_fingerprint("another-key"));
    }
}
