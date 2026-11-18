//! Emit one host-filled format. Static ink is composited in design order;
//! native fields are additive, so a later opaque layer cannot erase one.
use super::{
    template::{parts, Part},
    *,
};
use std::fmt::Write;

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Rect {
    fn intersects(self, other: Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

impl DesignDocument {
    pub fn to_zpl_template(&self, dpmm: i32) -> Result<String> {
        self.validate(dpmm)?;
        let width = render::canvas_extent(self.width_mm, dpmm);
        let height = render::canvas_extent(self.height_mm, dpmm);
        ensure!(width >= 2, "ZPL print width must be at least two dots");
        let mut background = GrayImage::from_pixel(width, height, Luma([255]));
        let mut native = String::new();
        let mut bound: Vec<(Rect, Background, String)> = Vec::new();
        for (i, o) in self
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.style.visible)
        {
            let dots = |mm: f64| (mm * f64::from(dpmm)).round() as u32;
            let (w, h) = o.bounds_size();
            let rect = Rect {
                x: dots(o.x),
                y: dots(o.y),
                w: dots(w),
                h: dots(h),
            };
            let label = format!(
                "Object {} ({})",
                i + 1,
                if o.style.name.is_empty() {
                    format!("{:?}", o.kind)
                } else {
                    o.style.name.clone()
                }
            );
            if let Some((_, _, lower)) = bound.iter().find(|(r, background, _)| {
                r.intersects(rect)
                    && (o.style.background != Background::Transparent
                        || (!o.variable && *background == Background::Black))
            }) {
                anyhow::bail!("{label}: background or foreground ink covers variable {lower}. Move the layers apart or use transparent backgrounds.");
            }
            if !o.variable {
                render::paint(&mut background, o, self, dpmm).context(label)?;
                continue;
            }
            if o.style.background != Background::Transparent {
                let paper = if o.style.background == Background::Black {
                    0
                } else {
                    255
                };
                for y in rect.y..(rect.y + rect.h).min(height) {
                    for x in rect.x..(rect.x + rect.w).min(width) {
                        background.put_pixel(x, y, Luma([paper]));
                    }
                }
            }
            self.native_field(o, dpmm, &mut native)
                .with_context(|| label.clone())?;
            bound.push((rect, o.style.background, label));
        }
        ensure!(
            !bound.is_empty(),
            "Bind a visible text or barcode to a variable first"
        );
        let mut zpl = if background.as_raw().iter().any(|&p| p < 128) {
            let mut out = Self::bitmap_zpl(background, dpmm)?;
            let end = out
                .strip_suffix("^XZ\n")
                .context("Invalid graphic field ending")?
                .len();
            out.truncate(end);
            out
        } else {
            format!("^XA{}\n", Self::zpl_page(width, height, dpmm))
        };
        writeln!(zpl, "^CI28\n{native}^XZ")?;
        Ok(zpl)
    }

    fn native_field(&self, o: &DesignObject, dpmm: i32, out: &mut String) -> Result<()> {
        // The source carries only escaped literals and named slots. It never carries sample data.
        let mut expression = String::new();
        for part in parts(&o.text)? {
            match part {
                Part::Field(name) => write!(expression, "{{{{{name}}}}}")?,
                Part::Literal(text) => {
                    for byte in text.bytes() {
                        write!(expression, "_{byte:02X}")?;
                    }
                }
            }
        }
        let mut sample = o.clone();
        sample.text = o.sample_text(&self.samples)?;
        let dots = |mm: f64| (mm * f64::from(dpmm)).round() as u32;
        let (x, y, w, h) = (dots(o.x), dots(o.y), dots(o.width), dots(o.height));
        let orientation = match o.style.rotation {
            90 => 'R',
            180 => 'I',
            270 => 'B',
            _ => 'N',
        };
        let font_size = dots(o.font_mm).max(1);
        let font = if self.printer_font.is_empty() {
            format!("^A0{orientation},{font_size},{font_size}")
        } else {
            format!(
                "^A@{orientation},{font_size},{font_size},{}",
                self.printer_font
            )
        };
        // ^FO locates the top-left of the rotated field. Transform its local bounding rectangle.
        let origin = |left: u32, top: u32, width: u32, height: u32| {
            let (dx, dy) = match o.style.rotation {
                90 => (h.saturating_sub(top + height), left),
                180 => (
                    w.saturating_sub(left + width),
                    h.saturating_sub(top + height),
                ),
                270 => (top, w.saturating_sub(left + width)),
                _ => (left, top),
            };
            let reverse = if o.style.background == Background::Black {
                "^FR"
            } else {
                ""
            };
            format!("^FO{},{}{reverse}", x + dx, y + dy)
        };
        if o.kind == ObjectKind::Text {
            ensure!(
                !self.printer_font.is_empty() || sample.text.chars().all(|ch| ch as u32 <= 255),
                "Set the device font path for this Unicode text (for example E:MYFONT.TTF)"
            );
            ensure!(
                !self.printer_font.is_empty()
                    || !matches!(o.style.font, TextFont::BoldMonospace | TextFont::Monospace),
                "Select Document font and specify the matching printer font"
            );
            let step = (f64::from(font_size) * o.style.line_spacing).ceil().max(1.) as u32;
            let lines = sample.text.lines().count().max(1) as u32;
            let text_h = font_size + (lines - 1) * step;
            ensure!(text_h <= h, "Text height exceeds the object");
            ensure!(
                !o.style.wrap || o.style.vertical == VerticalAlign::Top,
                "Wrapped variable text requires top vertical alignment"
            );
            let top = render::vertical(i64::from(h - text_h), o.style.vertical) as u32;
            let use_block = o.style.wrap
                || lines > 1
                || o.style.rotation != 0
                || o.style.horizontal != HorizontalAlign::Left;
            let position = origin(0, top, w, if o.style.wrap { h } else { text_h });
            let mut commands = format!("{position}{font}");
            let kind = if use_block {
                let align = match o.style.horizontal {
                    HorizontalAlign::Left => 'L',
                    HorizontalAlign::Center => 'C',
                    HorizontalAlign::Right => 'R',
                };
                let max_lines = if o.style.wrap {
                    1 + (h - font_size) / step
                } else {
                    lines
                };
                write!(
                    commands,
                    "^FB{w},{max_lines},{},{align},0",
                    i64::from(step) - i64::from(font_size)
                )?;
                "block"
            } else {
                "text"
            };
            return field(out, kind, &commands, "", &expression);
        }
        // Limit automatic sizing to the device's representable whole-dot modules.
        let max_module = match o.kind {
            ObjectKind::Qr => 100,
            ObjectKind::DataMatrix => 32000,
            _ => 10,
        };
        let mut layout = symbols::layout(&sample, w, h, dpmm)?;
        if layout.module > max_module && sample.style.module_mm == 0. {
            sample.style.module_mm = f64::from(max_module) / f64::from(dpmm);
            layout = symbols::layout(&sample, w, h, dpmm)?;
        }
        let m = layout.module;
        ensure!(
            m <= max_module,
            "Module width exceeds {max_module} dots for this ZPL barcode"
        );
        let height = layout.target_h;
        // Zebra adds the current ^BY height to a ^FO-anchored QR. Set it below
        // and compensate here; the QR quiet zone always supplies the extra dot.
        let qr_offset = u32::from(o.kind == ObjectKind::Qr);
        let position = origin(
            layout.x as u32,
            layout.y as u32 - qr_offset,
            layout.source.width() * m,
            height,
        );
        let mut prefix = String::new();
        let (kind, command) = match o.kind {
            ObjectKind::Code128 => ("code128", format!("^BC{orientation},{height},N,N,N,A")),
            ObjectKind::Code39 => ("code39", format!("^B3{orientation},N,{height},N,N")),
            ObjectKind::Ean13 => ("ean13", format!("^BE{orientation},{height},N,N")),
            ObjectKind::Ean8 => ("ean8", format!("^B8{orientation},{height},N,N")),
            ObjectKind::Upca => ("upca", format!("^BU{orientation},{height},N,N,Y")),
            ObjectKind::Itf => ("itf", format!("^B2{orientation},{height},N,N,N")),
            ObjectKind::Qr => {
                ensure!(o.style.rotation == 0, "Native ZPL QR requires 0° rotation");
                prefix = format!("{:?}A,", o.style.qr_ecc);
                ("qr", format!("^BY2,3,1^BQN,2,{m}"))
            }
            ObjectKind::DataMatrix => {
                let shape = if o.style.matrix_shape == MatrixShape::Square {
                    1
                } else {
                    2
                };
                (
                    "datamatrix",
                    format!("^BX{orientation},{m},200,,,,|,{shape}"),
                )
            }
            ObjectKind::Pdf417 => (
                "pdf417",
                format!(
                    "^B7{orientation},{},{},{},,N",
                    m * 3,
                    o.style.pdf417_security,
                    if o.style.pdf417_columns == 0 {
                        String::new()
                    } else {
                        o.style.pdf417_columns.to_string()
                    }
                ),
            ),
            ObjectKind::Aztec => ("aztec", format!("^BO{orientation},{m},N,0,N")),
            _ => anyhow::bail!("Select a text or barcode object"),
        };
        let mut commands = position;
        if !o.kind.is_matrix() || o.kind == ObjectKind::Pdf417 {
            write!(commands, "^BY{m},3,1")?;
        }
        commands.push_str(&command);
        field(out, kind, &commands, &prefix, &expression)?;
        if layout.text_height > 0 {
            let position = origin(0, layout.text_y, w, layout.text_height);
            let commands = format!("{position}{font}^FB{w},1,0,C,0");
            field(out, &format!("{kind}-caption"), &commands, "", &expression)?;
        }
        Ok(())
    }
}

fn field(
    out: &mut String,
    kind: &str,
    commands: &str,
    prefix: &str,
    expression: &str,
) -> Result<()> {
    writeln!(
        out,
        "^FXLabelize field {kind}^FS\n{commands}^FH^FD{prefix}{expression}^FS"
    )?;
    Ok(())
}
