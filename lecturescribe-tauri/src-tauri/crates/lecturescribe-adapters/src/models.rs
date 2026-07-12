use chrono::Utc;
use lecturescribe_core::{
    AppError, ErrorCategory, ModelAvailability, ModelOption, ModelValidation, ModelValidationStatus,
};

pub const CURATED_MODELS: [CuratedModel; 2] = [
    CuratedModel {
        id: "gemini-3.1-flash-lite",
        label: "Gemini 3.1 Flash-Lite",
        description: "Recommended for speed and efficiency.",
        recommended: true,
    },
    CuratedModel {
        id: "gemini-3.5-flash",
        label: "Gemini 3.5 Flash",
        description: "Higher quality for demanding transcripts.",
        recommended: false,
    },
];

#[derive(Debug, Clone, Copy)]
pub struct CuratedModel {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeminiModelMetadata {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub supported_generation_methods: Vec<String>,
}

impl GeminiModelMetadata {
    pub fn supports_generate_content(&self) -> bool {
        self.supported_generation_methods
            .iter()
            .any(|method| method.eq_ignore_ascii_case("generateContent"))
    }
}

pub fn normalize_model_id(model: &str) -> String {
    model.trim().trim_start_matches("models/").to_string()
}

pub fn curated_model_options(
    metadata: &[GeminiModelMetadata],
    custom_model: Option<&str>,
) -> Result<Vec<ModelOption>, AppError> {
    let mut options = CURATED_MODELS
        .iter()
        .map(|model| {
            let available = metadata
                .iter()
                .find(|entry| entry.id == model.id)
                .is_some_and(GeminiModelMetadata::supports_generate_content);
            model_option(
                model.id,
                model.label,
                model.description,
                model.recommended,
                false,
                available,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let custom_model = custom_model
        .map(normalize_model_id)
        .filter(|model| !model.is_empty());
    if let Some(custom_model) = custom_model {
        if !CURATED_MODELS.iter().any(|model| model.id == custom_model) {
            let metadata = metadata.iter().find(|entry| entry.id == custom_model);
            options.push(model_option(
                &custom_model,
                metadata
                    .map(|entry| entry.display_name.as_str())
                    .filter(|label| !label.trim().is_empty())
                    .unwrap_or("Custom Gemini model"),
                metadata
                    .map(|entry| entry.description.as_str())
                    .filter(|description| !description.trim().is_empty())
                    .unwrap_or("Custom model requested for validation."),
                false,
                true,
                metadata.is_some_and(GeminiModelMetadata::supports_generate_content),
            )?);
        }
    }
    Ok(options)
}

pub fn supported_model_validation(model: &str) -> Result<ModelValidation, AppError> {
    model_validation(
        model,
        true,
        "This Gemini model is available and supports transcript generation.",
    )
}

pub fn successful_model_validation(
    model: &str,
    message: &str,
) -> Result<ModelValidation, AppError> {
    Ok(ModelValidation {
        model_id: normalize_model_id(model),
        availability: ModelAvailability::Available,
        status: ModelValidationStatus::Valid,
        message: message.to_string(),
        checked_at: Some(Utc::now()),
    })
}

pub fn unavailable_model_error(model: &str, exists: bool) -> AppError {
    let message = if exists {
        "This Gemini model does not support transcript generation."
    } else {
        "This Gemini model is not available for the configured API key."
    };
    AppError::new(
        if exists {
            "transcription_model_unsupported"
        } else {
            "transcription_model_unavailable"
        },
        ErrorCategory::Setup,
        message,
        format!(
            "Model `{}` was {}.",
            normalize_model_id(model),
            if exists {
                "listed without generateContent support"
            } else {
                "not returned by the Gemini Models API"
            }
        ),
    )
    .with_action("open_setup_model", "Choose model", "open_setup_model")
}

fn model_option(
    id: &str,
    label: &str,
    description: &str,
    recommended: bool,
    custom: bool,
    available: bool,
) -> Result<ModelOption, AppError> {
    let description = if available {
        description.to_string()
    } else {
        format!("{description} Not available for the configured API key.")
    };
    Ok(ModelOption {
        id: id.to_string(),
        display_name: label.to_string(),
        description,
        recommended,
        quality_label: if custom {
            "Custom".to_string()
        } else if recommended {
            "Recommended".to_string()
        } else {
            "Higher quality".to_string()
        },
    })
}

fn model_validation(model: &str, valid: bool, message: &str) -> Result<ModelValidation, AppError> {
    let (availability, status) = if valid {
        (ModelAvailability::Available, ModelValidationStatus::Valid)
    } else {
        (
            ModelAvailability::Unavailable,
            ModelValidationStatus::Invalid,
        )
    };
    Ok(ModelValidation {
        model_id: normalize_model_id(model),
        availability,
        status,
        message: message.to_string(),
        checked_at: Some(Utc::now()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, methods: &[&str]) -> GeminiModelMetadata {
        GeminiModelMetadata {
            id: id.to_string(),
            display_name: id.to_string(),
            description: String::new(),
            supported_generation_methods: methods.iter().map(|value| value.to_string()).collect(),
        }
    }

    #[test]
    fn curated_filtering_does_not_leak_the_full_catalog() {
        let models = vec![
            model("gemini-3.1-flash-lite", &["generateContent"]),
            model("gemini-3.5-flash", &["generateContent"]),
            model("gemini-noisy-experimental", &["generateContent"]),
        ];
        let options = curated_model_options(&models, None).unwrap();
        let ids = options
            .iter()
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["gemini-3.1-flash-lite", "gemini-3.5-flash"]);
    }

    #[test]
    fn invalid_custom_model_has_a_friendly_recovery_action() {
        let error = unavailable_model_error("gemini-not-real", false);

        assert_eq!(error.code, "transcription_model_unavailable");
        assert_eq!(error.recovery_actions[0].id, "open_setup_model");
        assert!(!error.technical_detail.contains("AIza"));
    }
}
