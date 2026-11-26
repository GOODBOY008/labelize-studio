//! Portable host-filled templates. Only declared byte ranges in the native
//! command stream are substituted; graphics and fixed fields remain opaque.
use super::{
    template::{parts, Part},
    *,
};
use std::{fmt::Write as _, path::Path, str::FromStr};
mod dpl;
mod native;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateLanguage {
    #[default]
    Zpl,
    Tspl,
    Epl,
    Cpcl,
    Dpl,
}
impl TemplateLanguage {
    pub const ALL: [Self; 5] = [Self::Zpl, Self::Tspl, Self::Epl, Self::Cpcl, Self::Dpl];
    pub fn from_template_path(path: &Path) -> Option<Self> {
        let stem = Path::new(path.file_stem()?);
        if !stem.extension()?.eq_ignore_ascii_case("tpl") {
            return None;
        }
        path.extension()?.to_str()?.parse().ok()
    }
    pub fn document_kind(self) -> crate::document::DocumentKind {
        use crate::document::DocumentKind;
        match self {
            Self::Zpl => DocumentKind::Zpl,
            Self::Tspl => DocumentKind::Tspl,
            Self::Epl => DocumentKind::Epl,
            Self::Cpcl => DocumentKind::Cpcl,
            Self::Dpl => DocumentKind::Dpl,
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zpl => "zpl",
            Self::Tspl => "tspl",
            Self::Epl => "epl",
            Self::Cpcl => "cpcl",
            Self::Dpl => "dpl",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Zpl => "ZPL",
            Self::Tspl => "TSPL/TSC",
            Self::Epl => "EPL2",
            Self::Cpcl => "CPCL",
            Self::Dpl => "DPL",
        }
    }
    pub fn supports_utf8(self) -> bool {
        matches!(self, Self::Zpl | Self::Tspl | Self::Cpcl)
    }
}
impl FromStr for TemplateLanguage {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "zpl" => Ok(Self::Zpl),
            "tspl" | "tsc" => Ok(Self::Tspl),
            "epl" | "epl2" => Ok(Self::Epl),
            "cpcl" => Ok(Self::Cpcl),
            "dpl" => Ok(Self::Dpl),
            _ => anyhow::bail!("Choose zpl, tspl, epl, cpcl or dpl"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateEncoding {
    #[default]
    #[serde(rename = "ascii")]
    Ascii,
    #[serde(rename = "utf-8")]
    Utf8,
}
impl TemplateEncoding {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ascii => "US-ASCII",
            Self::Utf8 => "UTF-8",
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrinterProfile {
    pub font: String,
    pub encoding: TemplateEncoding,
}
impl PrinterProfile {
    pub(super) fn validate(&self, lang: TemplateLanguage) -> Result<()> {
        ensure!(
            lang != TemplateLanguage::Zpl,
            "Use printer_font for the ZPL font"
        );
        ensure!(
            self.encoding == TemplateEncoding::Ascii || lang.supports_utf8(),
            "{}: unsupported device encoding {}",
            lang.name(),
            self.encoding.name()
        );
        ensure!(
            self.font.len() <= 64
                && self
                    .font
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
            "{}: invalid device font identifier",
            lang.name()
        );
        Ok(())
    }
}

pub(super) fn expression(text: &str) -> Result<String> {
    let mut encoded = String::new();
    for part in parts(text)? {
        match part {
            Part::Field(name) => write!(encoded, "{{{{{name}}}}}")?,
            Part::Literal(text) => {
                for b in text.bytes() {
                    write!(encoded, "_{b:02X}")?;
                }
            }
        }
    }
    Ok(encoded)
}

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
impl Rect {
    fn object(o: &DesignObject, dpmm: i32) -> Self {
        let dots = |v: f64| (v * f64::from(dpmm)).round() as u32;
        let (w, h) = o.bounds_size();
        Self {
            x: dots(o.x),
            y: dots(o.y),
            w: dots(w),
            h: dots(h),
        }
    }

    fn intersects(self, b: Self) -> bool {
        self.x < b.x + b.w && b.x < self.x + self.w && self.y < b.y + b.h && b.y < self.y + self.h
    }
}

impl DesignDocument {
    pub fn printer_profile(&self, language: TemplateLanguage) -> PrinterProfile {
        if language == TemplateLanguage::Zpl {
            PrinterProfile {
                font: self.printer_font.clone(),
                encoding: TemplateEncoding::Utf8,
            }
        } else {
            self.printer_profiles
                .get(&language)
                .cloned()
                .unwrap_or_default()
        }
    }
    pub fn set_printer_profile(
        &mut self,
        language: TemplateLanguage,
        profile: PrinterProfile,
    ) -> Result<()> {
        if language == TemplateLanguage::Zpl {
            self.printer_font = profile.font;
        } else {
            profile.validate(language)?;
            if profile == PrinterProfile::default() {
                self.printer_profiles.remove(&language);
            } else {
                self.printer_profiles.insert(language, profile);
            }
        }
        self.version = self.portable_version();
        Ok(())
    }
    pub fn to_print_template(&self, language: TemplateLanguage, dpmm: i32) -> Result<Vec<u8>> {
        if language == TemplateLanguage::Zpl {
            return Ok(self.to_zpl_template(dpmm)?.into_bytes());
        }
        self.validate(dpmm)?;
        let profile = self.printer_profile(language);
        let fixed = if language == TemplateLanguage::Dpl {
            dpl::native_background(self, &profile, dpmm)
        } else {
            None
        };
        let mut job = NativeJob::default();
        let background = self.template_layers(dpmm, language, fixed.is_some(), |i, o| {
            native::object(&mut job, language, &profile, self, i, o, dpmm)
        })?;
        let (mut prefix, suffix) = self.native_frame(language, &profile, dpmm, background)?;
        if let Some(commands) = fixed {
            prefix.extend(commands);
        }
        Ok(job.finish(language, profile.encoding, prefix, &suffix))
    }

    pub(super) fn template_layers(
        &self,
        dpmm: i32,
        language: TemplateLanguage,
        native_background: bool,
        mut emit: impl FnMut(usize, &DesignObject) -> Result<()>,
    ) -> Result<GrayImage> {
        let width = render::canvas_extent(self.width_mm, dpmm);
        let height = render::canvas_extent(self.height_mm, dpmm);
        let mut canvas = GrayImage::from_pixel(width, height, Luma([255]));
        let mut painted: Vec<(Rect, &DesignObject, String)> = Vec::new();
        let mut problems = Vec::new();
        let mut any_variable = false;
        for (i, o) in self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.style.visible)
        {
            let rect = Rect::object(o, dpmm);
            let label = format!(
                "Object {} ({})",
                i + 1,
                if o.style.name.is_empty() {
                    format!("{:?}", o.kind)
                } else {
                    o.style.name.clone()
                }
            );
            any_variable |= o.variable;
            if let Some((_, _, lower)) = painted.iter().find(|(bounds, prior, _)| {
                bounds.intersects(rect)
                    && (o.style.background != Background::Transparent
                        || !o.variable && prior.style.background == Background::Black)
            }) {
                problems.push(format!("{label}: background or foreground ink covers {lower}; use a transparent background or move the layers apart"));
            }
            let result = if o.variable {
                if !native_background && o.style.background != Background::Transparent {
                    let paper = if o.style.background == Background::Black {
                        0
                    } else {
                        255
                    };
                    for y in rect.y..(rect.y + rect.h).min(height) {
                        for x in rect.x..(rect.x + rect.w).min(width) {
                            canvas.put_pixel(x, y, Luma([paper]));
                        }
                    }
                }
                emit(i + 1, o)
            } else if native_background {
                Ok(())
            } else {
                render::paint(&mut canvas, o, self, dpmm)
            };
            if let Err(e) = result {
                problems.push(format!("{label}: {e:#}"));
            }
            if o.variable {
                painted.push((rect, o, label));
            }
        }
        if !any_variable {
            problems.push("Bind a visible text or barcode to a variable first".into());
        }
        ensure!(
            problems.is_empty(),
            "{} template:\n{}",
            language.name(),
            problems.join("\n")
        );
        Ok(canvas)
    }

    fn native_frame(
        &self,
        lang: TemplateLanguage,
        profile: &PrinterProfile,
        dpmm: i32,
        image: GrayImage,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let (width, height) = image.dimensions();
        if lang == TemplateLanguage::Dpl {
            return dpl::frame(&image, dpmm);
        }
        let ink = image.as_raw().iter().any(|&p| p < 128);
        match lang {
            TemplateLanguage::Tspl => {
                let end = b"PRINT 1,1\r\n";
                let mut head = if ink {
                    let rgba = image::DynamicImage::ImageLuma8(image).into_rgba8();
                    let mut bytes = labelize::image_to_tspl(
                        &rgba,
                        &labelize::ReverseOptions {
                            dpmm,
                            ..Default::default()
                        },
                    )?;
                    ensure!(bytes.ends_with(end), "Unexpected TSPL bitmap terminator");
                    bytes.truncate(bytes.len() - end.len());
                    bytes
                } else {
                    format!("SIZE {} mm,{} mm\r\nCLS\r\n", self.width_mm, self.height_mm)
                        .into_bytes()
                };
                let code = if profile.encoding == TemplateEncoding::Utf8 {
                    "UTF-8"
                } else {
                    "1252"
                };
                // Reset positioning before the BITMAP as well as the native fields.
                head.splice(
                    0..0,
                    b"DIRECTION 1\r\nREFERENCE 0,0\r\nSHIFT 0\r\n"
                        .iter()
                        .copied(),
                );
                head.extend_from_slice(format!("CODEPAGE {code}\r\n").as_bytes());
                Ok((head, end.to_vec()))
            }
            TemplateLanguage::Epl | TemplateLanguage::Cpcl => {
                let epl = lang == TemplateLanguage::Epl;
                let dpi = if dpmm == 8 { 200 } else { 300 };
                let mut prefix = if epl {
                    format!("N\r\nq{width}\r\nQ{height},0\r\nR0,0\r\n")
                } else {
                    format!(
                        "! 0 {dpi} {dpi} {height} 1\r\nPAGE-WIDTH {width}\r\nBARCODE-TEXT OFF\r\n"
                    )
                }
                .into_bytes();
                if !epl && profile.encoding == TemplateEncoding::Utf8 {
                    prefix.extend_from_slice(b"COUNTRY UTF-8\r\n");
                }
                if ink {
                    let graphic = pack(&image, 0, 0);
                    let row = graphic.row_bytes;
                    let header = if epl {
                        format!("GW0,0,{row},{height},")
                    } else {
                        format!("CG {row} {height} 0 0 ")
                    };
                    prefix.extend_from_slice(header.as_bytes());
                    prefix.extend(graphic.data);
                    prefix.extend_from_slice(b"\r\n");
                }
                Ok((
                    prefix,
                    if epl {
                        b"P1\r\n".to_vec()
                    } else {
                        b"PRINT\r\n".to_vec()
                    },
                ))
            }
            TemplateLanguage::Dpl | TemplateLanguage::Zpl => unreachable!(),
        }
    }
}

struct Slot {
    offset: usize,
    length: usize,
    object: usize,
    kind: String,
    codec: String,
    params: String,
}

#[derive(Default)]
struct NativeJob {
    body: Vec<u8>,
    slots: Vec<Slot>,
}
impl NativeJob {
    fn raw(&mut self, text: &str) {
        self.body.extend_from_slice(text.as_bytes());
    }
    fn field(
        &mut self,
        index: usize,
        kind: &str,
        codec: &str,
        expr: &str,
        o: &DesignObject,
    ) -> Result<()> {
        let token = expression(expr)?;
        let params = match o.kind {
            ObjectKind::Qr => format!("{:?}", o.style.qr_ecc),
            ObjectKind::DataMatrix => match o.style.matrix_shape {
                MatrixShape::Square => "square".into(),
                MatrixShape::Rectangle => "rectangle".into(),
            },
            ObjectKind::Pdf417 => format!("{},{}", o.style.pdf417_columns, o.style.pdf417_security),
            _ => "-".into(),
        };
        self.slots.push(Slot {
            offset: self.body.len(),
            length: token.len(),
            object: index,
            kind: kind.to_owned(),
            codec: codec.to_owned(),
            params,
        });
        self.raw(&token);
        Ok(())
    }
    fn finish(
        self,
        language: TemplateLanguage,
        encoding: TemplateEncoding,
        prefix: Vec<u8>,
        suffix: &[u8],
    ) -> Vec<u8> {
        let mut header = format!(
            "LABELIZE-TPL/1 {} {}\n",
            language.extension(),
            encoding.name()
        );
        for slot in self.slots {
            writeln!(
                header,
                "{} {} {} {} {} {}",
                prefix.len() + slot.offset,
                slot.length,
                slot.object,
                slot.kind,
                slot.codec,
                slot.params
            )
            .unwrap();
        }
        header.push('\n');
        let mut bytes = header.into_bytes();
        bytes.extend(prefix);
        bytes.extend(self.body);
        bytes.extend(suffix);
        bytes
    }
}
