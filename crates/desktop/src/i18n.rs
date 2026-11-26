use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::Path};

mod diagnostic_keys;
mod message;
pub use message::Message;

struct LanguagePack {
    tag: &'static str,
    name: &'static str,
    aliases: &'static [&'static str],
    messages: &'static [&'static str],
}

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

impl Language {
    fn pack(self) -> &'static LanguagePack {
        &PACKS[self as usize]
    }
    pub fn locale(self) -> &'static str {
        self.pack().tag
    }
    pub fn name(self) -> &'static str {
        self.pack().name
    }
    pub fn text(self, key: Text) -> &'static str {
        self.pack().messages[key as usize]
    }

    fn matching(tag: &str) -> Option<Self> {
        let normalized = tag.split(['.', '@']).next()?.replace('_', "-");
        let mut candidate = normalized.as_str();
        loop {
            if let Some(lang) = Self::ALL.iter().copied().find(|lang| {
                let pack = lang.pack();
                pack.tag.eq_ignore_ascii_case(candidate)
                    || pack
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(candidate))
            }) {
                return Some(lang);
            }
            candidate = candidate.rsplit_once('-')?.0;
        }
    }

    fn preferred(tags: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        tags.into_iter()
            .find_map(|tag| Self::matching(tag.as_ref()))
            .unwrap_or(Self::English)
    }

    pub fn format(self, key: Text, args: &[(&str, &str)]) -> String {
        let mut template = self.text(key);
        let mut output = String::new();
        while let Some((literal, tail)) = template.split_once("%{") {
            let (name, rest) = tail.split_once('}').expect("validated locale pack");
            output.push_str(literal);
            if let Some((_, value)) = args.iter().find(|(arg, _)| *arg == name) {
                output.push_str(value);
            } else {
                debug_assert!(false, "Missing {name} in {key:?}");
                output.push_str(&format!("%{{{name}}}"));
            }
            template = rest;
        }
        output.push_str(template);
        output
    }

    /// Next language in menu order, retained for two-language callers.
    pub fn other(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }

    pub fn page_count(self, count: usize) -> String {
        self.format(
            if count == 1 {
                Text::PageOne
            } else {
                Text::PageOther
            },
            &[("count", &count.to_string())],
        )
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::preferred(sys_locale::get_locales())
    }
}
impl Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.locale())
    }
}
impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let tag = String::deserialize(deserializer)?;
        Ok(Self::matching(&tag).unwrap_or_default())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub language: Language,
    pub automatic_updates: bool,
    pub last_update_check: Option<i64>,
    pub update_source: Option<labelize_printer_updater::InstallSource>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::default(),
            automatic_updates: true,
            last_update_check: None,
            update_source: None,
        }
    }
}

impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Cannot read desktop preferences"),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).context("Cannot open desktop preferences"),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        labelize_printer_core::paths::atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod preference_tests {
    use super::*;
    #[test]
    fn old_language_preferences_keep_update_defaults() {
        let p: Preferences = serde_json::from_str(r#"{"language":"en"}"#).unwrap();
        assert!(p.automatic_updates);
        assert!(p.last_update_check.is_none());
        let mut p = p;
        p.automatic_updates = false;
        p.last_update_check = Some(123);
        let switched = Preferences {
            language: p.language.other(),
            ..p.clone()
        };
        let roundtrip: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&switched).unwrap()).unwrap();
        assert!(!roundtrip.automatic_updates);
        assert_eq!(roundtrip.last_update_check, Some(123));
    }
}

#[cfg(test)]
mod locale_tests {
    use super::*;

    #[test]
    fn first_run_uses_the_first_supported_system_language() {
        for (list, expected) in [
            (vec!["ja-JP", "zh-Hans-SG", "en-US"], Language::Chinese),
            (vec!["de", "en_GB.UTF-8", "zh"], Language::English),
            (vec!["ZH_cn.UTF-8"], Language::Chinese),
            (vec!["zh-HK"], Language::Chinese),
            (vec!["fr-FR", "de-DE"], Language::English),
            (vec![], Language::English),
        ] {
            assert_eq!(Language::preferred(list), expected);
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("desktop.json");
        assert_eq!(
            Preferences::load(&path).unwrap().language,
            Language::preferred(sys_locale::get_locales())
        );
        let mut prefs = Preferences::default();
        for &language in Language::ALL {
            prefs.language = language;
            prefs.save(&path).unwrap();
            assert_eq!(Preferences::load(&path).unwrap().language, language);
            assert_eq!(serde_json::to_value(language).unwrap(), language.locale());
        }
        fs::write(
            &path,
            r#"{"language":"unavailable","automatic_updates":false,"last_update_check":42}"#,
        )
        .unwrap();
        let recovered = Preferences::load(&path).unwrap();
        assert_eq!(recovered.language, Language::default());
        assert!(!recovered.automatic_updates);
        assert_eq!(recovered.last_update_check, Some(42));
    }

    #[test]
    fn missing_variable_hint_recognizes_current_and_saved_diagnostics() {
        use labelize_printer_core::design::{DesignObject, ObjectKind};

        let mut field = DesignObject::new(ObjectKind::Text);
        field.variable = true;
        field.text = "Plain text".into();
        let diagnostic = field.variable_names().unwrap_err().to_string();
        assert_eq!(
            diagnostic,
            Language::English.text(Text::DiagnosticVariableExample)
        );
        for &language in Language::ALL {
            let hint = language.text(Text::DiagnosticVariableExample);
            assert_eq!(language.diagnostic(&diagnostic), hint);
            assert_eq!(
                language.diagnostic("Enter a variable such as {{order_no}}"),
                hint
            );
        }
    }

    #[test]
    fn switching_pack_preserves_field_names_and_external_details() {
        let field = "order_%{field}{{number}}";
        let error = Message::formatted(Text::RecordFieldString, [("field", field.to_string())]);
        assert_eq!(
            error.display(Language::English),
            format!("Field {field} must be a string.")
        );
        assert_eq!(
            error.display(Language::Chinese),
            format!("字段 {field} 的值必须是字符串。")
        );
        assert_eq!(
            Language::Chinese
                .diagnostic("Error: Parse error: No variable fields in this template\n"),
            "解析错误：此模板中没有可填充的变量字段。"
        );
        assert_eq!(
            Language::Chinese.diagnostic("Object 3 (order_no): Incorrect check digit"),
            "对象 3（order_no）：校验位不正确。"
        );
        assert_eq!(
            Language::Chinese.diagnostic("Expected 12 digits with an optional check digit"),
            "请输入 12 位数字，可在末尾加上校验位。"
        );
        assert_eq!(
            Language::English.diagnostic("Value cannot be encoded as US-ASCII"),
            "Value cannot be encoded as US-ASCII"
        );
        assert_eq!(
            Language::Chinese.diagnostic("future driver error /tmp/订单.zpl"),
            "future driver error /tmp/订单.zpl"
        );
        assert_eq!(
            Language::Chinese.field_kind("ean13-caption"),
            "EAN-13 可读文字"
        );
        assert_eq!(Language::Chinese.field_kind("block"), "文本块");
        assert_eq!(
            Language::Chinese.diagnostic(
                "Parse error: Invalid field expression at line 6: Invalid template variable name"
            ),
            "解析错误：第 6 行的字段表达式无效：模板变量名无效。"
        );
        assert_eq!(
            Language::Chinese.diagnostic("Error: unknown OS failure  \n"),
            "Error: unknown OS failure  \n"
        );
        assert_eq!(Language::English.page_count(1), "1 page");
        assert_eq!(Language::English.page_count(2), "2 pages");
        assert_eq!(Language::Chinese.page_count(1), "1 页");
    }
}
