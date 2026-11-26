use super::DocumentKind;
use crate::design::{DesignDocument, TemplateLanguage};
use anyhow::{ensure, Result};
use labelize::{Language, PrintTemplate, TemplateValues};
use serde::{Deserialize, Serialize};

pub const FILLED_JOB: &str = "filled.job";
pub const TEMPLATE_REPORT: &str = "template.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub language: TemplateLanguage,
    pub values: TemplateValues,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateProblem {
    pub fields: Vec<String>,
    pub object: usize,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateReport {
    pub fields: Vec<String>,
    pub problems: Vec<TemplateProblem>,
}

impl TemplateRecord {
    pub(super) fn prepare(
        &self,
        bytes: &[u8],
        kind: DocumentKind,
        dpmm: i32,
    ) -> Result<(TemplateReport, Option<Vec<u8>>)> {
        let exported;
        let source = if kind == DocumentKind::Design {
            exported = serde_json::from_slice::<DesignDocument>(bytes)?
                .to_print_template(self.language, dpmm)?;
            &exported
        } else {
            ensure!(
                kind == self.language.document_kind(),
                "Template language mismatch"
            );
            bytes
        };
        let language =
            Language::from_name(self.language.extension()).expect("known printer language");
        let template = PrintTemplate::load(language, source)?;
        let report = TemplateReport {
            fields: template.fields().iter().map(|f| f.name.clone()).collect(),
            problems: template
                .validate(&self.values)
                .into_iter()
                .map(|p| TemplateProblem {
                    fields: p.fields,
                    object: p.usage.object,
                    kind: p.usage.kind,
                    message: p.message,
                })
                .collect(),
        };
        let job = if report.problems.is_empty() {
            let job = template.fill(&self.values)?;
            ensure!(
                job.len() as u64 <= crate::MAX_INPUT_BYTES,
                "Filled job exceeds 64 MiB"
            );
            Some(job)
        } else {
            None
        };
        Ok((report, job))
    }
}
