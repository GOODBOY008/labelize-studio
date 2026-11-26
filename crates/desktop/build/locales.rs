//! Compile the shipped language packs into typed, offline desktop resources.
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    fs,
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pack {
    variant: String,
    name: String,
    aliases: Vec<String>,
    messages: BTreeMap<String, String>,
}

fn placeholders(text: &str) -> BTreeSet<&str> {
    text.split("%{")
        .skip(1)
        .map(|tail| {
            let (name, _) = tail
                .split_once('}')
                .expect("Unclosed translation placeholder");
            assert!(
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "Invalid placeholder in {text}"
            );
            name
        })
        .collect()
}

fn identifier(value: &str) -> bool {
    value != "Self"
        && value.starts_with(|c: char| c.is_ascii_uppercase())
        && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn compile(out: &Path) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut packs = BTreeMap::new();
    for entry in fs::read_dir(&directory).expect("Read desktop locales") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let tag = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let pack: Pack = serde_json::from_slice(&fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        packs.insert(tag, pack);
    }
    let english = &packs.get("en").expect("English fallback pack").messages;
    for key in english.keys() {
        assert!(identifier(key), "Invalid translation key: {key}");
    }
    let mut names = BTreeSet::new();
    let mut variants = BTreeSet::new();
    for (tag, pack) in &packs {
        assert!(
            !pack.name.trim().is_empty()
                && identifier(&pack.variant)
                && variants.insert(&pack.variant),
            "{tag}: missing native name, invalid or duplicate variant"
        );
        for name in std::iter::once(tag).chain(&pack.aliases) {
            assert!(name.split('-').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric())), "Invalid locale tag: {name}");
            assert!(
                names.insert(name.to_ascii_lowercase()),
                "Duplicate locale tag or alias: {name}"
            );
        }
        for (key, reference) in english {
            let value = pack
                .messages
                .get(key)
                .unwrap_or_else(|| panic!("{tag}: missing {key}"));
            assert!(!value.trim().is_empty(), "{tag}: empty {key}");
            assert_eq!(
                placeholders(reference),
                placeholders(value),
                "{tag}: placeholders in {key}"
            );
        }
        assert_eq!(
            pack.messages.len(),
            english.len(),
            "{tag}: unexpected translation key"
        );
    }

    let mut code = String::new();
    writeln!(
        code,
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)] pub enum Text {{"
    )
    .unwrap();
    for key in english.keys() {
        writeln!(code, "{key},").unwrap();
    }
    writeln!(
        code,
        "}}\n#[derive(Clone, Copy, Debug, Eq, PartialEq)] pub enum Language {{"
    )
    .unwrap();
    for pack in packs.values() {
        writeln!(code, "{},", pack.variant).unwrap();
    }
    writeln!(
        code,
        "}}\nimpl Language {{ pub const ALL: &'static [Self] = &["
    )
    .unwrap();
    for pack in packs.values() {
        writeln!(code, "Self::{},", pack.variant).unwrap();
    }
    writeln!(code, "]; }}\nstatic PACKS: &[LanguagePack] = &[").unwrap();
    for (tag, pack) in &packs {
        writeln!(
            code,
            "LanguagePack {{ tag: {tag:?}, name: {:?}, aliases: &{:?}, messages: &[",
            pack.name, pack.aliases
        )
        .unwrap();
        for message in pack.messages.values() {
            writeln!(code, "{message:?},").unwrap();
        }
        writeln!(code, "]}},").unwrap();
    }
    writeln!(code, "];").unwrap();
    fs::write(out.join("locales.rs"), code).expect("Write compiled locales");
}
