use super::{native::Page, *};

// Native records can be moved to the background only when no later layer
// erases their ink. Otherwise the shared compositor resolves the fixed layers.
pub(super) fn native_background(
    doc: &DesignDocument,
    profile: &PrinterProfile,
    dpmm: i32,
) -> Option<Vec<u8>> {
    let mut job = NativeJob::default();
    let mut earlier = Vec::new();
    for (index, o) in doc
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| o.style.visible)
    {
        let bounds = Rect::object(o, dpmm);
        if o.style.background != Background::Transparent
            && earlier
                .iter()
                .any(|&(rect, lower)| ink_intersects(rect, lower, bounds, dpmm))
        {
            return None;
        }
        if o.variable {
            continue;
        }
        if matches!(o.kind, ObjectKind::Image | ObjectKind::Ellipse)
            || o.kind == ObjectKind::Rectangle && o.style.corner_radius_mm > 0.
            || o.kind == ObjectKind::Text
                && o.style.font == TextFont::Document
                && !doc.font.is_empty()
        {
            return None;
        }
        native::object(
            &mut job,
            TemplateLanguage::Dpl,
            profile,
            doc,
            index + 1,
            o,
            dpmm,
        )
        .ok()?;
        earlier.push((bounds, o));
    }
    Some(job.body)
}

pub(super) fn frame(image: &GrayImage, dpmm: i32) -> Result<(Vec<u8>, Vec<u8>)> {
    let (w, h) = image.dimensions();
    let mut header = b"\x02L\rm\rD11\r".to_vec();
    let mut footer = b"E\r".to_vec();
    let mut downloads = Vec::new();
    let mut left = w;
    let mut top = h;
    let mut right = 0;
    let mut bottom = 0;
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[0] < 128 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 {
        return Ok((header, footer));
    }

    // Pad only as far as the nearest whole-mm anchor so a 300-dpi label
    // retains its original dots after DPL's tenths-of-a-mm conversion.
    let d = dpmm as u32;
    left = (left / d).min(999) * d;
    bottom = h - ((h - bottom) / d).min(999) * d;
    let width = right - left;
    let row_bytes = width.div_ceil(8);
    let max_rows = (2 * 1024 * 1024 / row_bytes) / d * d;
    while bottom > top {
        let height = (bottom - top).min(max_rows);
        let y = bottom - height;
        let tile = image::imageops::crop_imm(image, left, y, width, height).to_image();
        let bmp = monochrome_bmp(&tile);
        let mut crc = flate2::Crc::new();
        crc.update(&bmp);
        let name = format!("LS{:08X}{bottom:04X}", crc.sum());
        // SOH D disables immediate-command interpretation for the binary download.
        downloads.extend_from_slice(format!("\x01D\x02IDb{name}\r").as_bytes());
        downloads.extend(bmp);
        let row = units(h - bottom, dpmm)?;
        let column = units(left, dpmm)?;
        header.extend_from_slice(format!("1Y11000{row:04}{column:04}{name}\r").as_bytes());
        footer.extend_from_slice(format!("\x02xDG{name}\r").as_bytes());
        bottom = y;
    }
    downloads.extend(header);
    Ok((downloads, footer))
}

// Windows 1-bit BMP: white palette index 0, black index 1, bottom-up rows padded to four bytes.
fn monochrome_bmp(image: &GrayImage) -> Vec<u8> {
    let packed = pack(image, 0, 0);
    let row = packed.row_bytes as usize;
    let stride = row.next_multiple_of(4);
    let data_bytes = stride * image.height() as usize;
    const PIXELS: usize = 14 + 40 + 8;
    let mut bmp = vec![0; PIXELS + data_bytes];
    let length = bmp.len() as u32;
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&length.to_le_bytes());
    bmp[10..14].copy_from_slice(&(PIXELS as u32).to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&image.width().to_le_bytes());
    bmp[22..26].copy_from_slice(&image.height().to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&1u16.to_le_bytes());
    bmp[34..38].copy_from_slice(&(data_bytes as u32).to_le_bytes());
    bmp[54..57].fill(255);
    for (src, dst) in packed
        .data
        .chunks_exact(row)
        .rev()
        .zip(bmp[PIXELS..].chunks_exact_mut(stride))
    {
        dst[..row].copy_from_slice(src);
    }
    bmp
}

// DPL metric positions count tenths of a millimetre upwards from the media bottom.
fn units(dots: u32, dpmm: i32) -> Result<u32> {
    let units = (f64::from(dots) * 10. / f64::from(dpmm)).round() as u32;
    ensure!(units <= 9999, "DPL position or size exceeds four digits");
    Ok(units)
}
fn multiplier(n: u32) -> Result<char> {
    ensure!((1..=24).contains(&n), "DPL multiplier must be 1–24");
    Ok(if n < 10 {
        (b'0' + n as u8) as char
    } else {
        (b'A' + (n - 10) as u8) as char
    })
}
fn coords(p: Rect, page: Page) -> Result<String> {
    let bottom = page
        .height
        .checked_sub(p.y + p.h)
        .context("DPL field falls outside the media")?;
    Ok(format!(
        "{:04}{:04}",
        units(bottom, page.dpmm)?,
        units(p.x, page.dpmm)?
    ))
}
pub(super) fn text(mut p: Rect, page: Page, r: u16, id: &str, factor: u32) -> Result<String> {
    if matches!(r, 90 | 270) {
        p.h = p.w;
    }
    let scale = multiplier(factor)?;
    Ok(format!(
        "{}{id}{scale}{scale}000{}",
        1 + r / 90,
        coords(p, page)?
    ))
}
pub(super) fn shape(job: &mut NativeJob, o: &DesignObject, page: Page) -> Result<()> {
    let dots = |mm: f64| (mm * f64::from(page.dpmm)).round() as u32;
    let (w, h) = o.bounds_size();
    let mut p = Rect {
        x: dots(o.x),
        y: dots(o.y),
        w: dots(w),
        h: dots(h),
    };
    let stroke = dots(o.stroke_mm).max(1);
    let filled = o.kind == ObjectKind::Line || o.style.filled;
    if o.kind == ObjectKind::Line {
        p = line_bounds(p, o, stroke).context("DPL native lines must be horizontal or vertical")?;
    }
    let (w, h) = (units(p.w, page.dpmm)?.max(1), units(p.h, page.dpmm)?.max(1));
    let pen = units(stroke, page.dpmm)?.max(1);
    job.raw(&format!("1X11000{}", coords(p, page)?));
    job.raw(&if filled {
        format!("l{w:04}{h:04}\r")
    } else {
        format!("b{w:04}{h:04}{pen:04}{pen:04}\r")
    });
    Ok(())
}
fn line_bounds(mut rect: Rect, o: &DesignObject, stroke: u32) -> Option<Rect> {
    let horizontal = match o.style.line_direction {
        LineDirection::Horizontal => true,
        LineDirection::Vertical => false,
        _ => return None,
    } ^ matches!(o.style.rotation, 90 | 270);
    if horizontal {
        rect.y += rect.h.saturating_sub(stroke) / 2;
        rect.h = stroke;
    } else {
        rect.x += rect.w.saturating_sub(stroke) / 2;
        rect.w = stroke;
    }
    Some(rect)
}

// A white field inside a previously drawn frame has no earlier ink to erase.
fn ink_intersects(rect: Rect, o: &DesignObject, upper: Rect, dpmm: i32) -> bool {
    if !rect.intersects(upper) {
        return false;
    }
    let stroke = (o.stroke_mm * f64::from(dpmm)).round().max(1.) as u32;
    if o.kind == ObjectKind::Line {
        return line_bounds(rect, o, stroke).is_none_or(|line| line.intersects(upper));
    }
    if o.kind == ObjectKind::Rectangle && !o.style.filled {
        return upper.x < rect.x + stroke
            || upper.y < rect.y + stroke
            || upper.x + upper.w > (rect.x + rect.w).saturating_sub(stroke)
            || upper.y + upper.h > (rect.y + rect.h).saturating_sub(stroke);
    }
    true
}

fn module_units(dots: u32, dpmm: i32, units_per_mm: u32) -> u32 {
    (f64::from(dots * units_per_mm) / f64::from(dpmm))
        .round()
        .max(1.) as u32
}

pub(super) fn module_fits(kind: ObjectKind, m: u32, dpmm: i32) -> bool {
    let exact = |dots, units_per_mm| {
        let units = module_units(dots, dpmm, units_per_mm);
        units <= 24
            && (f64::from(units) * f64::from(dpmm) / f64::from(units_per_mm)).round() as u32 == dots
    };
    match kind {
        ObjectKind::Qr | ObjectKind::Aztec => exact(m, 10),
        ObjectKind::DataMatrix => exact(m, 8),
        ObjectKind::Pdf417 => exact(m, 8) && exact(3 * m, 8),
        _ => true,
    }
}

pub(super) fn barcode(
    o: &DesignObject,
    p: Rect,
    page: Page,
    m: u32,
    height: u32,
) -> Result<(String, String, &'static str)> {
    use ObjectKind::*;
    let r = 1 + o.style.rotation / 90;
    let (typ, c, d, bar_h, options, codec) = match o.kind {
        Qr | Aztec => {
            let u = module_units(m, page.dpmm, 10);
            let (typ, opt) = if o.kind == Qr {
                ("W1D", format!("{:?}A,", o.style.qr_ecc))
            } else {
                ("W1f", String::new())
            };
            let codec = if o.kind == Aztec { "dpl-aztec" } else { "line" };
            (typ, u, u, 0, opt, codec.to_owned())
        }
        DataMatrix | Pdf417 => {
            let base = |dots| module_units(dots, page.dpmm, 8);
            let dm = o.kind == DataMatrix;
            ensure!(
                !dm || o.style.matrix_shape == MatrixShape::Square,
                "DPL Data Matrix requires a square symbol"
            );
            let opts = if dm {
                "2006000000".to_owned()
            } else {
                format!(
                    "F{}0000{:02}",
                    o.style.pdf417_security, o.style.pdf417_columns
                )
            };
            (
                if dm { "W1C" } else { "Z" },
                base(m),
                if dm { base(m) } else { base(3 * m) },
                0,
                String::new(),
                format!("count4:{opts}"),
            )
        }
        _ => {
            let typ = match o.kind {
                Code128 => "e",
                Code39 => "a",
                Ean13 => "f",
                Ean8 => "g",
                Upca => "b",
                Itf => "d",
                _ => anyhow::bail!("Unsupported DPL barcode"),
            };
            let wide = if matches!(o.kind, Code39 | Itf) {
                m * 3
            } else {
                m
            };
            (
                typ,
                wide,
                m,
                units(height, page.dpmm)?,
                String::new(),
                "line".to_owned(),
            )
        }
    };
    ensure!(bar_h <= 999, "DPL bar height exceeds the three-digit field");
    let cmd = format!(
        "{r}{typ}{}{}{bar_h:03}{}{options}",
        multiplier(c)?,
        multiplier(d)?,
        coords(p, page)?
    );
    Ok((cmd, codec, "\r"))
}
