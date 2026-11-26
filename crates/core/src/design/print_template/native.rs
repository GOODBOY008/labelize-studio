use super::*;
use ObjectKind::*;

#[derive(Clone, Copy)]
pub(super) struct Page {
    pub dpmm: i32,
    pub height: u32,
}
impl Page {
    fn dots(self, v: f64) -> u32 {
        (v * f64::from(self.dpmm)).round() as u32
    }
}

fn locate(o: &DesignObject, page: Page, l: u32, t: u32, w: u32, h: u32) -> Rect {
    let (ow, oh) = (page.dots(o.width), page.dots(o.height));
    let (dx, dy, w, h) = match o.style.rotation {
        90 => (oh.saturating_sub(t + h), l, h, w),
        180 => (ow.saturating_sub(l + w), oh.saturating_sub(t + h), w, h),
        270 => (t, ow.saturating_sub(l + w), h, w),
        _ => (l, t, w, h),
    };
    Rect {
        x: page.dots(o.x) + dx,
        y: page.dots(o.y) + dy,
        w,
        h,
    }
}

pub(super) fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        Text => "text",
        Code128 => "code128",
        Code39 => "code39",
        Ean13 => "ean13",
        Ean8 => "ean8",
        Upca => "upca",
        Itf => "itf",
        Qr => "qr",
        DataMatrix => "datamatrix",
        Pdf417 => "pdf417",
        Aztec => "aztec",
        _ => "graphic",
    }
}

struct Field<'a> {
    lang: TemplateLanguage,
    profile: &'a PrinterProfile,
    index: usize,
    o: &'a DesignObject,
    page: Page,
}

pub(super) fn object(
    job: &mut NativeJob,
    lang: TemplateLanguage,
    profile: &PrinterProfile,
    doc: &DesignDocument,
    index: usize,
    o: &DesignObject,
    dpmm: i32,
) -> Result<()> {
    ensure!(
        o.style.background != Background::Black,
        "Reverse-color fields are unavailable for this target"
    );
    let page = Page {
        dpmm,
        height: render::canvas_extent(doc.height_mm, dpmm),
    };
    if o.kind.is_shape() {
        return dpl::shape(job, o, page);
    }
    let mut sample = o.clone();
    if o.variable {
        sample.text = o.sample_text(&doc.samples)?;
    }
    if profile.encoding == TemplateEncoding::Ascii {
        ensure!(
            sample.text.is_ascii(),
            "Sample requires a device font and an encoding that supports its characters"
        );
    }
    let ctx = Field {
        lang,
        profile,
        index,
        o,
        page,
    };
    if o.kind == Text {
        return ctx.text(job, &sample.text, false, None);
    }
    ctx.barcode(job, sample)
}

impl Field<'_> {
    fn text(
        &self,
        job: &mut NativeJob,
        sample: &str,
        caption: bool,
        caption_y: Option<u32>,
    ) -> Result<()> {
        let o = self.o;
        ensure!(
            o.style.font == TextFont::Document || !self.profile.font.is_empty(),
            "Choose Document font or configure a matching device font"
        );
        ensure!(
            !o.style.wrap && !sample.chars().any(char::is_control),
            "Use a single line for this printer text field"
        );
        let size = self.page.dots(o.font_mm).max(1);
        let (id, cell_w, cell_h) = device_font(self.lang, self.profile)?;
        let scale = if self.lang == TemplateLanguage::Dpl {
            f64::from(self.page.dpmm) / 8.
        } else {
            1.
        };
        let cell_height = f64::from(cell_h) * scale;
        let ideal = f64::from(size) / cell_height;
        let factor = (if caption {
            ideal.floor()
        } else {
            ideal.round()
        } as u32)
            .max(1);
        let limit = match self.lang {
            TemplateLanguage::Tspl | TemplateLanguage::Epl => 10,
            TemplateLanguage::Cpcl => 16,
            _ => 24,
        };
        ensure!(
            factor <= limit,
            "Font exceeds the supported device magnification"
        );
        let height = (cell_height * f64::from(factor)).round() as u32;
        let cells = if id.ends_with(".BF2") || id.ends_with(".CPF") {
            use ab_glyph::{Font, FontRef};
            use labelize::elements::font_face::FontFace;
            let font = FontRef::try_from_slice(FontFace::BitmapSquare.ttf_data())?;
            let em = font.units_per_em().context("Missing device font metrics")?;
            sample
                .chars()
                .map(|ch| f64::from(font.h_advance_unscaled(font.glyph_id(ch)) / em))
                .sum()
        } else {
            sample.chars().count() as f64
        };
        let width = (f64::from(cell_w * factor) * scale * cells).round() as u32;
        let (ow, oh) = (self.page.dots(o.width), self.page.dots(o.height));
        ensure!(
            width <= ow && height <= oh,
            "Sample text exceeds the object at this device font size"
        );
        let x = render::horizontal(
            i64::from(ow - width),
            if caption {
                HorizontalAlign::Center
            } else {
                o.style.horizontal
            },
        ) as u32;
        let y = caption_y
            .unwrap_or_else(|| render::vertical(i64::from(oh - height), o.style.vertical) as u32);
        ensure!(
            y + height <= oh + 1,
            "Device caption exceeds the object's height"
        );
        let pos = locate(o, self.page, x, y, width, height);
        let (x, y, r) = (pos.x, pos.y, o.style.rotation);
        let (head, tail, codec) = match self.lang {
            TemplateLanguage::Tspl => (
                format!("TEXT {x},{y},\"{id}\",{r},{factor},{factor},\""),
                "\"\r\n",
                "tspl",
            ),
            TemplateLanguage::Epl => (
                format!("A{x},{y},{},{id},{factor},{factor},N,\"", r / 90),
                "\"\r\n",
                "epl",
            ),
            TemplateLanguage::Cpcl => {
                let op = match r {
                    90 => "T270",
                    180 => "T180",
                    270 => "T90",
                    _ => "TEXT",
                };
                (
                    format!("SETMAG {factor} {factor}\r\n{op} {id} 0 {x} {y} "),
                    "\r\n",
                    "line",
                )
            }
            TemplateLanguage::Dpl => (dpl::text(pos, self.page, r, id, factor)?, "\r", "line"),
            _ => unreachable!(),
        };
        let kind = if caption {
            format!("{}-caption", kind_name(o.kind))
        } else {
            "text".into()
        };
        self.write(job, &kind, (head, codec.into(), tail), o, sample)
    }

    fn write(
        &self,
        job: &mut NativeJob,
        kind: &str,
        cmd: Command,
        settings: &DesignObject,
        sample: &str,
    ) -> Result<()> {
        let (head, codec, tail) = cmd;
        check_data(&codec, sample)?;
        job.raw(&head);
        if self.o.variable {
            job.field(self.index, kind, &codec, &self.o.text, settings)?;
        } else {
            job.body.extend(fixed_dpl_data(kind, &codec, sample)?);
        }
        job.raw(tail);
        Ok(())
    }

    fn barcode(&self, job: &mut NativeJob, mut sample: DesignObject) -> Result<()> {
        let o = self.o;
        let dpmm = self.page.dpmm;
        let (w, h) = (self.page.dots(o.width), self.page.dots(o.height));
        let max = match (self.lang, o.kind) {
            (TemplateLanguage::Tspl, Aztec) => 20,
            (TemplateLanguage::Epl, Pdf417) => 9,
            (TemplateLanguage::Cpcl, Qr) => 32,
            (TemplateLanguage::Dpl, Code39 | Itf) => 8,
            (TemplateLanguage::Dpl, Qr | Aztec) => (24. * f64::from(dpmm) / 10.).round() as u32,
            (TemplateLanguage::Dpl, DataMatrix) => (24 * dpmm / 8) as u32,
            (TemplateLanguage::Dpl, Pdf417) => dpmm as u32,
            (TemplateLanguage::Dpl, _) => 24,
            (TemplateLanguage::Epl, Aztec) => 55,
            (TemplateLanguage::Epl, DataMatrix) => 40,
            (TemplateLanguage::Epl, Qr) => 100,
            (_, _) => 10,
        };
        let mut layout = symbols::layout(&sample, w, h, dpmm)?;
        let fits_module = |m| {
            m <= max && (self.lang != TemplateLanguage::Dpl || dpl::module_fits(o.kind, m, dpmm))
        };
        if !fits_module(layout.module) && o.style.module_mm == 0. {
            let best = (1..=layout.module.min(max))
                .rev()
                .find(|&m| fits_module(m))
                .context("Barcode cannot fit at an available device module size")?;
            sample.style.module_mm = f64::from(best) / f64::from(dpmm);
            layout = symbols::layout(&sample, w, h, dpmm)?;
        }
        let m = layout.module;
        ensure!(fits_module(m), "This printer cannot represent a {m}-dot module at {dpmm} dots/mm; use automatic sizing or change the module width");
        let mut pos = locate(
            o,
            self.page,
            layout.x as u32,
            layout.y as u32,
            layout.source.width() * m,
            layout.target_h,
        );
        // A full PDF417 row has 17 modules per data column plus 69 for framing.
        // CPCL defaults to three columns; freeze the designer's automatic choice.
        if o.kind == Pdf417 && o.style.pdf417_columns == 0 {
            sample.style.pdf417_columns = ((layout.source.width() - 69) / 17) as u8;
        }
        // Retail symbols extend guard bars past the commanded data bars. Reserve
        // their extent inside the object, including when the field is rotated.
        let mut height = layout.target_h;
        if matches!(o.kind, Ean8 | Ean13 | Upca) {
            let guard = 12;
            ensure!(
                height > guard,
                "Barcode height is too small for the retail guard bars"
            );
            height -= guard;
            match o.style.rotation {
                90 => pos.x += guard,
                180 => pos.y += guard,
                _ => {}
            }
        }
        let cmd = match self.lang {
            TemplateLanguage::Tspl => tspl(&sample, pos, m, height)?,
            TemplateLanguage::Epl => epl(&sample, pos, m, height)?,
            TemplateLanguage::Cpcl => cpcl(&sample, pos, m, height)?,
            TemplateLanguage::Dpl => {
                // DPL's row is the base of the unrotated field, in bottom-up units.
                pos.h = layout.target_h + if o.kind == Qr { 8 * m } else { 0 };
                dpl::barcode(&sample, pos, self.page, m, height)?
            }
            _ => unreachable!(),
        };
        self.write(job, kind_name(o.kind), cmd, &sample, &sample.text)?;
        if layout.text_height > 0 {
            self.text(job, &layout.caption, true, Some(layout.text_y as u32))?;
        }
        Ok(())
    }
}

fn device_font(lang: TemplateLanguage, profile: &PrinterProfile) -> Result<(&str, u32, u32)> {
    use TemplateLanguage::*;
    let id = if profile.font.is_empty() {
        match lang {
            Tspl | Epl => "1",
            Cpcl => "0",
            Dpl => "2",
            _ => unreachable!(),
        }
    } else {
        &profile.font
    };
    let size = match (lang, id) {
        (Tspl, "1") => (8, 12),
        (Tspl, "2") => (12, 20),
        (Tspl, "3") => (16, 24),
        (Tspl, "4") => (24, 32),
        (Tspl, "5") => (32, 48),
        (Tspl, "6") => (14, 19),
        (Tspl, "7") => (21, 27),
        (Tspl, "8") => (14, 25),
        (Tspl, "TSS16.BF2" | "TST16.BF2" | "TTT16.BF2") => (16, 16),
        (Tspl, "TSS24.BF2" | "TST24.BF2" | "TTT24.BF2") => (24, 24),
        (Epl, "1") => (8, 12),
        (Epl, "2") => (10, 16),
        (Epl, "3") => (12, 20),
        (Epl, "4") => (14, 24),
        (Epl, "5") => (32, 48),
        (Cpcl, "0") => (5, 9),
        (Cpcl, "7") => (24, 24),
        (Cpcl, "GBUNSG16.CPF") => (16, 16),
        (Cpcl, "GBUNSG24.CPF") => (24, 24),
        (Dpl, "0") => (5, 7),
        (Dpl, "1") => (7, 13),
        (Dpl, "2") => (10, 18),
        _ => anyhow::bail!("Unverified {} device font '{id}'", lang.name()),
    };
    if profile.encoding == TemplateEncoding::Utf8 {
        ensure!(
            id.ends_with(".BF2") || id.ends_with(".CPF"),
            "Select a verified printer CJK font for UTF-8 text"
        );
    }
    Ok((id, size.0, size.1))
}

type Command = (String, String, &'static str);
fn tspl(o: &DesignObject, p: Rect, m: u32, h: u32) -> Result<Command> {
    let (x, y, r) = (p.x, p.y, o.style.rotation);
    let (width, height) = if matches!(r, 90 | 270) {
        (p.h, p.w)
    } else {
        (p.w, p.h)
    };
    let (head, codec, tail) = match o.kind {
        Qr => (
            format!("QRCODE {x},{y},{:?},{m},A,{r},M2,S7,\"", o.style.qr_ecc),
            "tspl",
            "\"\r\n",
        ),
        DataMatrix => {
            let shape = if o.style.matrix_shape == MatrixShape::Rectangle {
                "a1"
            } else {
                "a0"
            };
            (
                format!("DMATRIX {x},{y},{width},{height},x{m},r{r},{shape},\""),
                "tspl",
                "\"\r\n",
            )
        }
        Aztec => (
            format!("AZTEC {x},{y},{r},{m},0,0,0,1,0,"),
            "aztec-count",
            "\r\n",
        ),
        Pdf417 => (
            format!(
                "PDF417 {x},{y},{width},{h},{r},E{},C{},W{m},H{},",
                o.style.pdf417_security,
                o.style.pdf417_columns,
                3 * m
            ),
            "tspl-length",
            "\r\n",
        ),
        _ => {
            let typ = match o.kind {
                Code128 => "128",
                Code39 => "39",
                Ean13 => "EAN13",
                Ean8 => "EAN8",
                Upca => "UPCA",
                Itf => "25",
                _ => anyhow::bail!("Unsupported TSPL barcode"),
            };
            (
                format!("BARCODE {x},{y},\"{typ}\",{h},0,{r},{m},{},\"", m * 3),
                "tspl",
                "\"\r\n",
            )
        }
    };
    Ok((head, codec.into(), tail))
}
fn epl(o: &DesignObject, p: Rect, m: u32, h: u32) -> Result<Command> {
    let (x, y, r) = (p.x, p.y, o.style.rotation / 90);
    ensure!(
        !o.kind.is_matrix() || r == 0 || o.kind == Pdf417,
        "This EPL2 matrix field requires 0° rotation"
    );
    let cmd = match o.kind {
        Qr => format!("b{x},{y},Q,m2,s{m},e{:?},iA,\"", o.style.qr_ecc),
        DataMatrix => {
            ensure!(
                o.style.matrix_shape == MatrixShape::Square,
                "EPL2 Data Matrix requires a square symbol"
            );
            format!("b{x},{y},D,h{m},\"")
        }
        Aztec => format!("b{x},{y},A,d{m},\""),
        Pdf417 => {
            ensure!(
                m >= 2 && o.style.pdf417_security > 0,
                "EPL2 PDF417 requires at least 2 dots per module and security level 1–8"
            );
            format!(
                "b{x},{y},P,x{m},y{},s{},l{},o{r},\"",
                3 * m,
                o.style.pdf417_security,
                o.style.pdf417_columns
            )
        }
        _ => {
            let typ = match o.kind {
                Code128 => "1",
                Code39 => "3",
                Ean13 => "E30",
                Ean8 => "E80",
                Upca => "UA0",
                Itf => "2",
                _ => unreachable!(),
            };
            format!("B{x},{y},{r},{typ},{m},{},{h},N,\"", m * 3)
        }
    };
    Ok((cmd, "epl".into(), "\"\r\n"))
}
fn cpcl(o: &DesignObject, p: Rect, m: u32, h: u32) -> Result<Command> {
    let (x, y) = (p.x, p.y);
    let op = match o.style.rotation {
        0 => "BARCODE",
        270 => "VBARCODE",
        _ => anyhow::bail!("CPCL barcodes support 0° or 270° rotation"),
    };
    let (head, codec, end) = match o.kind {
        Qr => (
            format!("{op} QR {x} {y} M 2 U {m}\r\n{:?}A,", o.style.qr_ecc),
            "line",
            "\r\nENDQR\r\n",
        ),
        DataMatrix => {
            ensure!(
                o.style.matrix_shape == MatrixShape::Square,
                "CPCL Data Matrix requires a square symbol"
            );
            (
                format!("{op} DATAMATRIX {x} {y} H {m} S 200\r\n"),
                "dm-line",
                "\r\nENDDATAMATRIX\r\n",
            )
        }
        Pdf417 => (
            format!(
                "{op} PDF-417 {x} {y} XD {m} YD {} S {} C {}\r\n",
                3 * m,
                o.style.pdf417_security,
                o.style.pdf417_columns
            ),
            "pdf-line",
            "\r\nENDPDF\r\n",
        ),
        Aztec => (
            format!("{op} AZTEC {x} {y} XD {m} EC 0\r\n"),
            "aztec-line",
            "\r\nENDAZTEC\r\n",
        ),
        _ => {
            let typ = match o.kind {
                Code128 => "128",
                Code39 => "39",
                Ean13 => "EAN13",
                Ean8 => "EAN8",
                Upca => "UPCA",
                Itf => "I2OF5",
                _ => anyhow::bail!("Unsupported CPCL barcode"),
            };
            (format!("{op} {typ} {m} 3 {h} {x} {y} "), "line", "\r\n")
        }
    };
    Ok((head, codec.into(), end))
}

fn check_data(codec: &str, value: &str) -> Result<()> {
    // These codecs carry a byte count or encode control bytes as CPCL decimal escapes.
    if matches!(codec, "tspl-length" | "aztec-count" | "dm-line") {
        return Ok(());
    }
    ensure!(
        !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')),
        "Control characters and line separators cannot be encoded in this field"
    );
    match codec {
        "pdf-line" => ensure!(
            !value.trim().eq_ignore_ascii_case("ENDPDF"),
            "Value is the CPCL PDF417 terminator"
        ),
        "aztec-line" => ensure!(
            !value.trim().eq_ignore_ascii_case("ENDAZTEC"),
            "Value is the CPCL Aztec terminator"
        ),
        _ => (),
    }
    Ok(())
}

fn fixed_dpl_data(kind: &str, codec: &str, value: &str) -> Result<Vec<u8>> {
    let mut value = value.to_owned();
    if let Some(len) = match kind {
        "ean13" => Some(12),
        "ean8" => Some(7),
        "upca" => Some(11),
        _ => None,
    } {
        value.truncate(len);
    }
    if codec == "dpl-aztec" {
        value.insert_str(0, "0000"); // ECI disabled, automatic error correction.
    }
    if let Some(options) = codec.strip_prefix("count4:") {
        let count = options.len() + value.len();
        ensure!(count <= 9999, "DPL field exceeds four-digit count");
        value = format!("{count:04}{options}{value}");
    }
    Ok(value.into_bytes())
}
