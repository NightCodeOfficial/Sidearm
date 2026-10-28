use serde::Serialize;

use crate::config::ConfigStoreError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    /// English text; the frontend shows the translation of `key` when present.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<String>>,
    /// i18n key under `backend.` with its params (a [`crate::ui_text::UiText`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<&'static str>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<&'static str, String>,
}

impl CommandError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Option<Vec<String>>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details,
            key: None,
            params: Default::default(),
        }
    }

    /// An error whose text is a translatable [`crate::ui_text::UiText`].
    pub fn from_ui(code: impl Into<String>, text: crate::ui_text::UiText) -> Self {
        Self { code: code.into(), message: text.text, details: None, key: text.key, params: text.params }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error".into(),
            message: message.into(),
            details: None,
            key: None,
            params: Default::default(),
        }
    }
}

impl From<ConfigStoreError> for CommandError {
    fn from(value: ConfigStoreError) -> Self {
        match value {
            ConfigStoreError::ConfigDirectoryUnavailable(message) => Self {
                code: "config_directory_unavailable".into(),
                message,
                details: None,
                key: None,
                params: Default::default(),
            },
            ConfigStoreError::Io { path, message } => Self { key: None, params: Default::default(),
                code: "io_error".into(),
                message,
                details: path.map(|path| vec![path]),
            },
            ConfigStoreError::Parse { path, message } => Self { key: None, params: Default::default(),
                code: "parse_error".into(),
                message,
                details: Some(vec![path]),
            },
            ConfigStoreError::SchemaEngine(message) => Self {
                code: "schema_engine_error".into(),
                message,
                details: None,
                key: None,
                params: Default::default(),
            },
            ConfigStoreError::SchemaViolation { errors } => Self { key: None, params: Default::default(),
                code: "schema_violation".into(),
                message: "Config schema validation failed.".into(),
                details: Some(errors),
            },
            ConfigStoreError::Serialize(message) => Self {
                code: "serialize_error".into(),
                message,
                details: None,
                key: None,
                params: Default::default(),
            },
            ConfigStoreError::InvalidConfig { errors } => Self { key: None, params: Default::default(),
                code: "invalid_config".into(),
                message: "Config validation failed.".into(),
                details: Some(errors),
            },
            ConfigStoreError::ConcurrentModification => Self {
                code: "config_changed_on_disk".into(),
                message: "Settings were changed by another Sidearm instance; reloading from disk."
                    .into(),
                details: None,
                key: None,
                params: Default::default(),
            },
        }
    }
}
