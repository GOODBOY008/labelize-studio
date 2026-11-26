use super::{diagnostic_keys::diagnostic_key, Language, Text};

/// Store messages independently of the active locale so open tabs and notices
/// can redraw in another language without losing user input or rerunning a job.
#[derive(Debug, Clone)]
pub enum Message {
    Label(Text),
    Formatted(Text, Vec<(&'static str, String)>),
    Diagnostic(String),
    WithDetail(Text, String),
}

impl From<Text> for Message {
    fn from(value: Text) -> Self {
        Self::Label(value)
    }
}
impl From<String> for Message {
    fn from(value: String) -> Self {
        Self::Diagnostic(value)
    }
}

impl Message {
    pub fn formatted<const N: usize>(text: Text, args: [(&'static str, String); N]) -> Self {
        Self::Formatted(text, args.into())
    }

    pub fn display(&self, language: Language) -> String {
        match self {
            Self::Label(text) => language.text(*text).to_owned(),
            Self::Formatted(text, values) => {
                let args: Vec<_> = values
                    .iter()
                    .map(|(name, value)| (*name, value.as_str()))
                    .collect();
                language.format(*text, &args)
            }
            Self::Diagnostic(raw) => language.diagnostic(raw),
            Self::WithDetail(text, detail) => language.format(
                Text::ContextDetail,
                &[
                    ("summary", language.text(*text)),
                    ("detail", &language.diagnostic(detail)),
                ],
            ),
        }
    }
}

impl Language {
    /// Renderer and OS errors cross a process boundary. Translate known reasons
    /// at presentation time and preserve unfamiliar technical details verbatim.
    pub fn diagnostic(self, raw: &str) -> String {
        let normalized = raw.trim_end_matches(['\n', '\r']);
        let translated = normalized
            .lines()
            .map(|line| self.diagnostic_line(line))
            .collect::<Vec<_>>()
            .join("\n");
        if translated == normalized {
            raw.to_owned()
        } else {
            translated
        }
    }

    fn diagnostic_line(self, line: &str) -> String {
        let original = line;
        let line = line.strip_prefix("Error: ").unwrap_or(line);
        if let Some((summary, detail)) = line.split_once(": ") {
            let context = match summary {
                "Parse error" => Some(Text::ParseError),
                "Render error" => Some(Text::RenderError),
                "Encode error" => Some(Text::EncodeError),
                "IO error" => Some(Text::IoError),
                _ => diagnostic_key(summary),
            };
            if let Some(key) = context {
                return self.format(
                    Text::DiagnosticCause,
                    &[
                        ("summary", self.text(key)),
                        ("detail", &self.diagnostic_reason(detail)),
                    ],
                );
            }
        }
        let translated = self.diagnostic_reason(line);
        if translated == line {
            original.to_owned()
        } else {
            translated
        }
    }

    fn diagnostic_reason(self, reason: &str) -> String {
        for (prefix, key) in [
            ("Object ", Text::DiagnosticObject),
            ("ZPL field ", Text::DiagnosticZplField),
        ] {
            if let Some((index, rest)) =
                reason.strip_prefix(prefix).and_then(|s| s.split_once(" ("))
            {
                if let Some((name, cause)) = rest.rsplit_once("): ") {
                    if index.parse::<usize>().is_ok() {
                        let cause = self.diagnostic_leaf(cause);
                        return self
                            .format(key, &[("index", index), ("name", name), ("reason", &cause)]);
                    }
                }
            }
        }
        self.diagnostic_leaf(reason)
    }

    fn diagnostic_leaf(self, reason: &str) -> String {
        if let Some(key) = diagnostic_key(reason) {
            return self.text(key).to_owned();
        }
        // Only the fixed frames come from Labelize. Interpolate field names and
        // values as opaque text, including braces or non-ASCII characters.
        for (prefix, suffix, arg, key) in [
            (
                "Expected ",
                " digits with an optional check digit",
                "digits",
                Text::DiagnosticGtinDigits,
            ),
            (
                "Value is the CPCL ",
                " terminator",
                "tag",
                Text::DiagnosticCpclTerminator,
            ),
            (
                "Invalid variable name '",
                "'; use letters, digits and underscores",
                "name",
                Text::DiagnosticVariableName,
            ),
            (
                "Missing sample for '",
                "'",
                "name",
                Text::DiagnosticMissingSample,
            ),
            ("", " template:", "language", Text::TemplateIssues),
            ("Updated to ", "", "version", Text::UpdateInstalled),
            (
                "Resumed ",
                "; update was not installed",
                "version",
                Text::UpdateResumed,
            ),
            (
                "Invalid field directory at line ",
                "",
                "line",
                Text::DirectoryLine,
            ),
            (
                "Invalid field offset, length or object at line ",
                "",
                "line",
                Text::DirectoryOffset,
            ),
            (
                "Invalid or overlapping field range at line ",
                "",
                "line",
                Text::DirectoryRange,
            ),
            (
                "Unknown ZPL template field type ",
                "",
                "kind",
                Text::TemplateUnknownZplKind,
            ),
            (
                "Unknown linear barcode kind ",
                "",
                "kind",
                Text::TemplateUnknownLinear,
            ),
            ("Invalid caption ", "", "kind", Text::TemplateInvalidCaption),
            (
                "Invalid field kind or parameters: ",
                "",
                "values",
                Text::TemplateInvalidParams,
            ),
            (
                "Invalid PDF417 columns or correction level: ",
                "",
                "options",
                Text::InvalidPdfOptions,
            ),
            (
                "background or foreground ink covers ",
                "; use a transparent background or move the layers apart",
                "object",
                Text::TemplateOverlap,
            ),
        ] {
            if let Some(value) = reason
                .strip_prefix(prefix)
                .and_then(|s| s.strip_suffix(suffix))
            {
                return self.format(key, &[(arg, value)]);
            }
        }
        if let Some((line, cause)) = reason
            .strip_prefix("Invalid field expression at line ")
            .and_then(|s| s.split_once(": "))
        {
            let detail = diagnostic_key(cause).map_or(cause, |key| self.text(key));
            return self.format(
                Text::DirectoryExpression,
                &[("line", line), ("reason", detail)],
            );
        }
        if let Some((target, expected)) = reason
            .strip_prefix("Template language ")
            .and_then(|s| s.split_once(" does not match "))
        {
            return self.format(
                Text::TemplateTarget,
                &[("actual", target), ("expected", expected)],
            );
        }
        if let Some((target, charset)) = reason
            .strip_prefix("Unsupported ")
            .and_then(|s| s.split_once(" template encoding: "))
        {
            return self.format(
                Text::TemplateCharset,
                &[("target", target), ("charset", charset)],
            );
        }
        if let Some((module, dpmm)) = reason
            .strip_prefix("This printer cannot represent a ")
            .and_then(|s| {
                s.strip_suffix(" dots/mm; use automatic sizing or change the module width")
            })
            .and_then(|s| s.split_once("-dot module at "))
        {
            return self.format(
                Text::ModuleWidthUnsupported,
                &[("module", module), ("dpmm", dpmm)],
            );
        }
        if let Some((key, detail)) = reason
            .split_once(": ")
            .and_then(|(part, detail)| diagnostic_key(part).map(|key| (key, detail)))
        {
            return self.format(
                Text::DiagnosticCause,
                &[("summary", self.text(key)), ("detail", detail)],
            );
        }
        reason.to_owned()
    }

    pub fn field_kind(self, kind: &str) -> String {
        let (kind, caption) = kind
            .strip_suffix("-caption")
            .map_or((kind, false), |base| (base, true));
        let name = match kind {
            "text" => self.text(Text::ObjectText),
            "block" => self.text(Text::CountedText),
            "qr" => self.text(Text::ObjectQr),
            "code128" => "Code 128",
            "code39" => "Code 39",
            "ean8" => "EAN-8",
            "ean13" => "EAN-13",
            "upca" => "UPC-A",
            "itf" => "ITF",
            "datamatrix" => "Data Matrix",
            "pdf417" => "PDF417",
            "aztec" => "Aztec",
            other => other,
        };
        if caption {
            self.format(Text::BarcodeCaption, &[("kind", name)])
        } else {
            name.to_owned()
        }
    }
}
