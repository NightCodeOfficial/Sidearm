//! User-facing text from the backend: an i18n key the frontend translates
//! (`backend.<key>` in `src/i18n/locales/{en,ru}.json`, `{{name}}` placeholders
//! filled from `params`) plus the English text — what the file log shows and the
//! frontend's fallback for a missing key. Build one with [`ui_text!`]; a plain
//! `String`/`&str` converts into an untranslated text (no key).

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UiText {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<&'static str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<&'static str, String>,
}

impl UiText {
    pub fn keyed(key: &'static str, text: String) -> Self {
        Self { text, key: Some(key), params: BTreeMap::new() }
    }

    pub fn param(mut self, name: &'static str, value: impl ToString) -> Self {
        self.params.insert(name, value.to_string());
        self
    }
}

impl From<String> for UiText {
    fn from(text: String) -> Self {
        Self { text, ..Self::default() }
    }
}

impl From<&str> for UiText {
    fn from(text: &str) -> Self {
        text.to_string().into()
    }
}

impl From<&String> for UiText {
    fn from(text: &String) -> Self {
        text.clone().into()
    }
}

/// Translations of the `backend.*` keys in the UI language, sent by the
/// frontend (`set_native_texts`) for the surfaces it cannot render itself —
/// the tray menu, its tooltip and the OSD.
static NATIVE_TEXTS: std::sync::RwLock<Option<std::collections::HashMap<String, String>>> =
    std::sync::RwLock::new(None);

pub fn set_native_texts(texts: std::collections::HashMap<String, String>) {
    *NATIVE_TEXTS.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(texts);
}

/// `text` in the UI language for a native surface: the frontend's translation
/// of its key with `{{name}}` filled from its params, else the English text.
pub fn native(text: &UiText) -> String {
    let texts = NATIVE_TEXTS.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(template) = text.key.and_then(|key| texts.as_ref()?.get(key)) else {
        return text.text.clone();
    };
    text.params
        .iter()
        .fold(template.clone(), |out, (name, value)| out.replace(&format!("{{{{{name}}}}}"), value))
}

impl std::fmt::Display for UiText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// `ui_text!("capture.signalReceived", "Got signal `{key}`.", key = encoded)`:
/// the key (under `backend.` in the locales), the English text with inline
/// `{name}` placeholders, and each named value — formatted into the English
/// text and sent as a param for the translation (`{{name}}` in the locales).
#[macro_export]
macro_rules! ui_text {
    ($key:literal, $fmt:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        // `name = name` rebinds on purpose: the English text captures `{name}`.
        $(#[allow(clippy::redundant_locals)] let $name = $value;)*
        $crate::ui_text::UiText::keyed($key, format!($fmt))$(.param(stringify!($name), &$name))*
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_text_carries_key_english_and_params() {
        let count = 3;
        let text = crate::ui_text!("demo.steps", "Runs {count} steps for `{name}`.", count = count, name = "A");
        assert_eq!(text.text, "Runs 3 steps for `A`.");
        assert_eq!(text.key, Some("demo.steps"));
        assert_eq!(text.params.get("count").map(String::as_str), Some("3"));
        assert_eq!(text.params.get("name").map(String::as_str), Some("A"));
        assert_eq!(native(&text), "Runs 3 steps for `A`.", "no translation yet: English");
        set_native_texts([("demo.steps".to_string(), "{{count}} шага для «{{name}}»".to_string())].into());
        assert_eq!(native(&text), "3 шага для «A»");
        let plain: UiText = "plain".into();
        assert_eq!(serde_json::to_value(&plain).unwrap(), serde_json::json!({ "text": "plain" }));
    }
}
