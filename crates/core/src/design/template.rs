use super::*;
use std::{borrow::Cow, collections::BTreeSet};

pub(super) enum Part<'a> {
    Literal(&'a str),
    Field(&'a str),
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

pub(super) fn parts(mut text: &str) -> Result<Vec<Part<'_>>> {
    ensure!(text.len() <= 4096, "Variable expression exceeds 4096 bytes");
    let mut result = Vec::new();
    while let Some((literal, tail)) = text.split_once("{{") {
        ensure!(
            !literal.contains("}}"),
            "Unexpected closing braces in variable expression"
        );
        result.push(Part::Literal(literal));
        let (key, rest) = tail
            .split_once("}}")
            .context("Close the variable with }}")?;
        ensure!(
            identifier(key),
            "Invalid variable name '{key}'; use letters, digits and underscores"
        );
        result.push(Part::Field(key));
        text = rest;
    }
    ensure!(
        !text.contains("}}"),
        "Unexpected closing braces in variable expression"
    );
    ensure!(
        !result.is_empty(),
        "Enter a variable such as {}",
        "{{name}}"
    );
    result.push(Part::Literal(text));
    Ok(result)
}

impl DesignObject {
    pub fn variable_names(&self) -> Result<BTreeSet<String>> {
        if !self.variable {
            return Ok(BTreeSet::new());
        }
        Ok(parts(&self.text)?
            .into_iter()
            .filter_map(|part| match part {
                Part::Field(name) => Some(name.to_owned()),
                Part::Literal(_) => None,
            })
            .collect())
    }

    pub(super) fn sample_text(&self, samples: &BTreeMap<String, String>) -> Result<String> {
        let mut value = String::new();
        for part in parts(&self.text)? {
            value.push_str(match part {
                Part::Literal(text) => text,
                Part::Field(name) => samples
                    .get(name)
                    .with_context(|| format!("Missing sample for '{name}'"))?,
            });
            ensure!(value.len() <= 4096, "Filled content exceeds 4096 bytes");
        }
        Ok(value)
    }
}

impl DesignDocument {
    pub fn portable_version(&self) -> u32 {
        if !self.printer_profiles.is_empty() {
            TEMPLATE_VERSION
        } else if self.objects.iter().any(|o| o.variable)
            || !self.samples.is_empty()
            || !self.printer_font.is_empty()
        {
            3
        } else {
            2
        }
    }

    pub fn variable_names(&self) -> Result<BTreeSet<String>> {
        let mut names = BTreeSet::new();
        for (i, o) in self.objects.iter().enumerate().filter(|(_, o)| o.variable) {
            names.extend(
                o.variable_names()
                    .with_context(|| format!("Object {} ({})", i + 1, o.style.name))?,
            );
        }
        Ok(names)
    }

    pub fn set_variable(&mut self, index: usize, enabled: bool) -> Result<()> {
        let object = self.objects.get_mut(index).context("Select an object")?;
        ensure!(
            object.kind.has_content() && !object.style.locked,
            "Select an unlocked text or barcode"
        );
        if object.variable == enabled {
            return Ok(());
        }
        if enabled && parts(&object.text).is_err() {
            let key = (index + 1..)
                .map(|n| format!("field_{n}"))
                .find(|key| !self.samples.contains_key(key))
                .unwrap();
            self.samples.insert(
                key.clone(),
                std::mem::replace(&mut object.text, format!("{{{{{key}}}}}")),
            );
        } else if !enabled {
            object.text = object.sample_text(&self.samples)?;
        }
        object.variable = enabled;
        self.sync_variables()
    }

    /// Reconcile the shared sample editor after a binding is added, renamed or deleted.
    pub fn sync_variables(&mut self) -> Result<()> {
        let names = self.variable_names()?;
        self.samples.retain(|name, _| names.contains(name));
        for name in names {
            self.samples.entry(name).or_default();
        }
        if self.samples.is_empty() {
            self.printer_font.clear();
        }
        self.version = self.portable_version();
        Ok(())
    }

    pub(super) fn validate_bindings(&self) -> Result<()> {
        ensure!(
            self.printer_profiles.is_empty() || self.version >= 4,
            "Printer profiles require a version 4 design"
        );
        for (&language, profile) in &self.printer_profiles {
            profile.validate(language)?;
        }
        ensure!(
            self.version >= 3 || self.portable_version() == 2,
            "Variable fields require a version 3 design"
        );
        ensure!(
            self.samples
                .iter()
                .all(|(k, v)| identifier(k) && k.len() <= 4096 && v.len() <= 4096),
            "Invalid sample name or sample exceeds 4096 bytes"
        );
        if !self.printer_font.is_empty() {
            let path = self.printer_font.as_bytes();
            ensure!(
                path.len() >= 3
                    && path.len() <= 128
                    && b"REBAZ".contains(&path[0])
                    && path[1] == b':'
                    && path[2..]
                        .iter()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(c)),
                "Use a printer font path such as E:MYFONT.TTF"
            );
        }
        for (i, o) in self.objects.iter().enumerate().filter(|(_, o)| o.variable) {
            ensure!(
                o.kind.has_content(),
                "Object {} cannot bind variables",
                i + 1
            );
            parts(&o.text).with_context(|| format!("Object {} ({})", i + 1, o.style.name))?;
        }
        Ok(())
    }

    pub(super) fn with_samples(&self) -> Result<Cow<'_, DesignDocument>> {
        if !self.objects.iter().any(|o| o.variable && o.style.visible) {
            return Ok(Cow::Borrowed(self));
        }
        let mut result = self.clone();
        for (i, o) in result
            .objects
            .iter_mut()
            .enumerate()
            .filter(|(_, o)| o.variable && o.style.visible)
        {
            o.text = o
                .sample_text(&self.samples)
                .with_context(|| format!("Object {} ({})", i + 1, o.style.name))?;
            o.variable = false;
        }
        Ok(Cow::Owned(result))
    }
}
