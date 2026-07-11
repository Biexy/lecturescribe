use lecturescribe_core::{AppError, ErrorCategory};

const SERVICE: &str = "LectureScribe";
const GEMINI_ACCOUNT: &str = "gemini_api_key";

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
        }
    }

    pub fn configured(&self) -> bool {
        self.gemini_key().ok().flatten().is_some()
    }
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
