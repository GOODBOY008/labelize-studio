#![cfg(feature = "render")]
use image::GrayImage;
use labelize::{Language, PrintTemplate, TemplateValues};
use labelize_printer_core::{
    design::*,
    document::{self, DocumentKind},
    RenderMode, Settings,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn shipping() -> DesignDocument {
    serde_json::from_str(include_str!(
        "../../../examples/print-templates/shipping.labelize.json"
    ))
    .unwrap()
}
fn symbol(kind: ObjectKind, value: &str) -> DesignDocument {
    let mut o = DesignObject::new(kind);
    o.text = "{{value}}".into();
    o.variable = true;
    o.x = 5.;
    o.y = 8.;
    o.width = if kind.is_matrix() && kind != ObjectKind::Pdf417 {
        36.
    } else {
        84.
    };
    o.height = 36.;
    o.style.module_mm = 0.5;
    if kind.is_matrix() {
        o.style.horizontal = HorizontalAlign::Center;
        o.style.vertical = VerticalAlign::Middle;
    }
    DesignDocument {
        version: 3,
        width_mm: 100.,
        height_mm: 60.,
        objects: vec![o],
        samples: [("value".into(), value.into())].into(),
        ..Default::default()
    }
}
fn load_template(lang: TemplateLanguage, bytes: &[u8]) -> PrintTemplate {
    let language = Language::from_name(lang.extension()).unwrap();
    PrintTemplate::load(language, bytes).unwrap_or_else(|e| panic!("{lang:?}: {e}"))
}
fn printed(bytes: &[u8], lang: TemplateLanguage, dpmm: i32, width: f64, height: f64) -> GrayImage {
    let kind = lang.document_kind();
    let mut page = None;
    document::render(
        bytes,
        kind,
        &Settings {
            dpmm,
            width_mm: width,
            height_mm: height,
            output_dir: std::env::temp_dir(),
            ..Default::default()
        },
        |p| {
            assert!(page.is_none(), "one job must yield one label");
            page = Some(p.pixels);
            Ok(())
        },
    )
    .unwrap_or_else(|e| panic!("{lang:?}: {e:#}"));
    page.expect("one printable label")
}
#[test]
fn language_profiles_round_trip_at_the_lowest_portable_version() {
    let mut d = shipping();
    assert_eq!(d.portable_version(), 3);
    let p = PrinterProfile {
        font: "TSS24.BF2".into(),
        encoding: TemplateEncoding::Utf8,
    };
    d.set_printer_profile(TemplateLanguage::Tspl, p.clone())
        .unwrap();
    assert_eq!(d.version, 4);
    let decoded: DesignDocument = serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    assert_eq!(d, decoded);
    assert_eq!(decoded.printer_profile(TemplateLanguage::Tspl), p);
    assert!(d.set_printer_profile(TemplateLanguage::Epl, p).is_err());
    d.version = 3;
    assert!(d.validate(8).is_err());
    d.version = 4;
    d.set_printer_profile(TemplateLanguage::Tspl, Default::default())
        .unwrap();
    assert_eq!(d.version, 3);
    assert!(d.printer_profiles.is_empty());
    for o in &mut d.objects {
        o.variable = false;
    }
    d.sync_variables().unwrap();
    assert_eq!(d.version, 2);
}
#[test]
fn preflight_reports_all_incompatible_visible_objects() {
    let mut d = symbol(ObjectKind::DataMatrix, "LOT-12");
    d.objects[0].style.matrix_shape = MatrixShape::Rectangle;
    let mut second = d.objects[0].clone();
    second.kind = ObjectKind::Code128;
    second.style.name = "sideways order".into();
    second.style.rotation = 90;
    second.style.module_mm = 0.;
    second.style.background = Background::Transparent;
    d.objects.push(second);
    let msg = format!(
        "{:#}",
        d.to_print_template(TemplateLanguage::Cpcl, 8).unwrap_err()
    );
    for part in [
        "Object 1",
        "square",
        "Object 2",
        "sideways order",
        "rotation",
    ] {
        assert!(msg.contains(part), "{msg}");
    }
    let mut d = shipping();
    let mut broken = DesignObject::new(ObjectKind::Image);
    broken.style.name = "missing logo".into();
    d.objects.push(broken);
    assert!(format!(
        "{:#}",
        d.to_print_template(TemplateLanguage::Dpl, 8).unwrap_err()
    )
    .contains("missing logo"));
    d.objects.last_mut().unwrap().style.visible = false;
    assert!(d.to_print_template(TemplateLanguage::Dpl, 8).is_ok());
}
#[test]
fn bundled_shipping_templates_match_export_and_dpl_allows_ink_inside_frames() {
    let mut d = shipping();
    for lang in TemplateLanguage::ALL {
        let file = examples().join(format!("shipping.tpl.{}", lang.extension()));
        let bytes = d.to_print_template(lang, 8).unwrap();
        assert_eq!(bytes, fs::read(file).unwrap());
        let template = load_template(lang, &bytes);
        assert_eq!(template.fields()[0].name, "order_no");
        assert_eq!(template.fields()[0].uses.len(), 3);
        assert_eq!(template.fields()[1].name, "product");
        assert_eq!(template.validate(&TemplateValues::new()).len(), 4);
        assert!(template.fill(&d.samples).is_ok(), "{lang:?}");
    }
    let artwork: DesignDocument = serde_json::from_str(include_str!(
        "../../../examples/print-templates/shipping-logo.labelize.json"
    ))
    .unwrap();
    assert_eq!(
        artwork.to_print_template(TemplateLanguage::Dpl, 8).unwrap(),
        fs::read(examples().join("shipping-logo.tpl.dpl")).unwrap()
    );
    let mut frame = DesignObject::new(ObjectKind::Rectangle);
    frame.x = 0.;
    frame.y = 0.;
    frame.width = d.width_mm;
    frame.height = d.height_mm;
    frame.style.filled = false;
    d.objects.insert(0, frame);
    assert!(d.to_print_template(TemplateLanguage::Dpl, 8).is_ok());
    d.objects[0].style.filled = true;
    let masked = d.to_print_template(TemplateLanguage::Dpl, 8).unwrap();
    assert!(masked.windows(5).any(|s| s == b"\x02IDbL"));
}

#[test]
fn samples_accept_literal_delimiters_and_check_device_rotation() {
    let mut d = shipping();
    d.samples.insert("product".into(), "A\"B\\".into());
    assert!(d.to_print_template(TemplateLanguage::Epl, 8).is_ok());
    assert!(d.to_print_template(TemplateLanguage::Tspl, 8).is_ok());
    let mut d = symbol(ObjectKind::Qr, "SO-00123");
    d.objects[0].style.rotation = 90;
    assert!(d.to_print_template(TemplateLanguage::Tspl, 8).is_ok());
    assert!(d.to_print_template(TemplateLanguage::Cpcl, 8).is_err());
    d.objects[0].style.rotation = 270;
    assert!(d.to_print_template(TemplateLanguage::Cpcl, 8).is_ok());
}

#[test]
fn dpl_module_sizes_must_be_representable_at_the_selected_resolution() {
    let mut d = symbol(ObjectKind::DataMatrix, "SO-00123");
    d.objects[0].style.module_mm = 4. / 12.;
    assert!(format!(
        "{:#}",
        d.to_print_template(TemplateLanguage::Dpl, 12).unwrap_err()
    )
    .contains("4-dot module"));
    d.objects[0].style.module_mm = 0.;
    assert!(d.to_print_template(TemplateLanguage::Dpl, 12).is_ok());
}

#[test]
fn mixed_width_chinese_font_samples_fit_their_native_cells() {
    let mut d = symbol(ObjectKind::Text, "A保B温");
    let o = &mut d.objects[0];
    o.width = 9.;
    o.height = 4.;
    o.font_mm = 3.;
    o.style.wrap = false;
    for (lang, font) in [
        (TemplateLanguage::Tspl, "TSS24.BF2"),
        (TemplateLanguage::Cpcl, "GBUNSG24.CPF"),
    ] {
        d.set_printer_profile(
            lang,
            PrinterProfile {
                font: font.into(),
                encoding: TemplateEncoding::Utf8,
            },
        )
        .unwrap();
        assert!(
            d.to_print_template(lang, 8).is_ok(),
            "{font} uses 12 + 24 + 12 + 24 dots"
        );
        d.objects[0].width = 8.875;
        assert!(
            d.to_print_template(lang, 8).is_err(),
            "{font} overflows by one dot"
        );
        d.objects[0].width = 9.;
    }
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/print-templates")
}
fn object_crop(page: &GrayImage, object: &DesignObject, dpmm: i32) -> GrayImage {
    let dot = |mm: f64| (mm * f64::from(dpmm)).round() as u32;
    let (w, h) = object.bounds_size();
    image::imageops::crop_imm(page, dot(object.x), dot(object.y), dot(w), dot(h)).to_image()
}

fn fill_order(lang: TemplateLanguage, template: &[u8], order: &str, product: &str) -> Vec<u8> {
    let values = [
        ("order_no".into(), order.into()),
        ("product".into(), product.into()),
    ]
    .into();
    load_template(lang, template).fill(&values).unwrap()
}

#[test]
fn dpl_artwork_and_variable_fields() {
    let lang = TemplateLanguage::Dpl;
    let design: DesignDocument = serde_json::from_str(include_str!(
        "../../../examples/print-templates/shipping-logo.labelize.json"
    ))
    .unwrap();
    let mut logo = design.clone();
    logo.objects.retain(|o| !o.variable);
    logo.sync_variables().unwrap();
    for dpmm in [8, 12] {
        let plain = shipping().to_print_template(lang, dpmm).unwrap();
        let with = design.to_print_template(lang, dpmm).unwrap();
        let ink = logo
            .render_pixels(dpmm, RenderMode::Raster, 1, false)
            .unwrap();
        assert!(ink.as_raw().contains(&0));
        let mut batch = Vec::new();
        for (order, product) in [
            ("SO-00042", "Thermal labels"),
            ("SO-00043", "Return {{lot}}"),
        ] {
            let job = fill_order(lang, &with, order, product);
            assert!(job.starts_with(b"\x01D\x02IDbLS"));
            let bmp = job.windows(2).position(|s| s == b"BM").unwrap();
            assert_eq!(&job[bmp + 28..bmp + 30], &1u16.to_le_bytes());
            let actual = printed(&job, lang, dpmm, design.width_mm, design.height_mm);
            let reference = fill_order(lang, &plain, order, product);
            let mut expected = printed(&reference, lang, dpmm, design.width_mm, design.height_mm);
            for (pixel, mask) in expected.pixels_mut().zip(ink.pixels()) {
                pixel[0] = pixel[0].min(mask[0]);
            }
            assert!(
                actual == expected,
                "DPL logo and filled fields at {dpmm} dpmm"
            );
            batch.extend(job);
        }
        let pages = document::parse(
            &batch,
            DocumentKind::Dpl,
            &Settings {
                dpmm,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(pages.len(), 2);
    }
}

#[test]
fn chinese_device_font_templates() {
    for (lang, fonts) in [
        (TemplateLanguage::Tspl, ["TSS16.BF2", "TSS24.BF2"]),
        (TemplateLanguage::Cpcl, ["GBUNSG16.CPF", "GBUNSG24.CPF"]),
    ] {
        for font in fonts {
            let mut d = shipping();
            d.set_printer_profile(
                lang,
                PrinterProfile {
                    font: font.into(),
                    encoding: TemplateEncoding::Utf8,
                },
            )
            .unwrap();
            for dpmm in [8, 12] {
                let template = d.to_print_template(lang, dpmm).unwrap();
                let job = fill_order(lang, &template, "SO-00042", "保温杯");
                let label = printed(&job, lang, dpmm, d.width_mm, d.height_mm);
                let product = object_crop(&label, &d.objects[1], dpmm);
                assert!(
                    product.as_raw().contains(&0),
                    "{font} {dpmm} missing product glyphs"
                );
                let (pdf, pages) = document::vector_pdf(
                    &job,
                    lang.document_kind(),
                    &Settings {
                        dpmm,
                        width_mm: d.width_mm,
                        height_mm: d.height_mm,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(pages, 1);
                let text = lopdf::Document::load_mem(&pdf)
                    .unwrap()
                    .extract_text(&[1])
                    .unwrap();
                assert!(
                    text.replace('\n', "").contains("保温杯"),
                    "{font} {dpmm}: {text:?}"
                );
            }
        }
    }
}

#[test]
fn legacy_chinese_files_preserve_the_text_and_its_printed_glyphs() {
    use labelize::elements::label_element::LabelElement;
    // GBK and GB18030 share these six bytes for the three characters.
    let value = [0xb1, 0xa3, 0xce, 0xc2, 0xb1, 0xad];
    let tspl = concat!(
        "SIZE 50 mm,30 mm\r\nDIRECTION 1\r\nCLS\r\nCODEPAGE 936\r\n",
        "TEXT 20,20,\"TSS24.BF2\",0,1,1,\""
    );
    let cpcl = "! 0 200 200 240 1\r\nCOUNTRY GB18030\r\nTEXT GBUNSG24.CPF 0 20 20 ";
    let jobs = [
        (TemplateLanguage::Tspl, tspl, "\"\r\nPRINT 1,1\r\n"),
        (TemplateLanguage::Cpcl, cpcl, "\r\nPRINT\r\n"),
    ];
    for (lang, head, tail) in jobs {
        let source = [head.as_bytes(), &value, tail.as_bytes()].concat();
        for dpmm in [8, 12] {
            let settings = Settings {
                dpmm,
                ..Default::default()
            };
            let labels = document::parse(&source, lang.document_kind(), &settings).unwrap();
            assert!(labels[0]
                .elements
                .iter()
                .any(|e| matches!(e, LabelElement::Text(t) if t.text == "保温杯")));
            assert!(printed(&source, lang, dpmm, 50., 30.).as_raw().contains(&0));
            let (pdf, _) = document::vector_pdf(&source, lang.document_kind(), &settings).unwrap();
            let text = lopdf::Document::load_mem(&pdf)
                .unwrap()
                .extract_text(&[1])
                .unwrap();
            assert!(
                text.replace('\n', "").contains("保温杯"),
                "{lang:?} {dpmm}: {text:?}"
            );
        }
    }
}

// Exercise the resource and public application entry point with complete orders.
fn check_print_order(lang: TemplateLanguage) {
    use labelize::elements::label_element::LabelElement;
    use rxing::BarcodeFormat::{CODE_128, QR_CODE};
    let d = shipping();
    let ext = lang.extension();
    let template = fs::read(examples().join(format!("shipping.tpl.{ext}"))).unwrap();
    let literal = if matches!(lang, TemplateLanguage::Tspl | TemplateLanguage::Epl) {
        r#"A\"B {{product}}\"#
    } else {
        "Return {{product}}"
    };
    for (order, product) in [("SO-00042", "Thermal labels"), ("SO-00043", literal)] {
        let bytes = fill_order(lang, &template, order, product);
        let page = printed(&bytes, lang, 8, d.width_mm, d.height_mm);
        let parsed = document::parse(&bytes, lang.document_kind(), &Settings::default()).unwrap();
        let lines: Vec<_> = parsed[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                LabelElement::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            lines,
            [format!("Order: {order}"), product.into()],
            "{lang:?} visible text"
        );
        for (object, format) in d.objects[2..].iter().zip([CODE_128, QR_CODE]) {
            let crop = object_crop(&page, object, 8);
            let decoded = rxing::helpers::detect_in_luma(
                crop.to_vec(),
                crop.width(),
                crop.height(),
                Some(format),
            )
            .unwrap();
            assert_eq!(decoded.getText(), order, "{lang:?} {format:?}");
        }
    }
}

#[test]
fn all_supported_barcodes_decode_at_both_resolutions() {
    use rxing::BarcodeFormat::*;
    let formats = [
        CODE_128,
        CODE_39,
        EAN_13,
        EAN_8,
        UPC_A,
        ITF,
        QR_CODE,
        DATA_MATRIX,
        PDF_417,
        AZTEC,
    ];
    let pairs = [
        ["SO-00012", "A>8^~_"],
        ["A-0001", "Z/002"],
        ["0123456789012", "5901234123457"],
        ["01234565", "96385074"],
        ["012345678905", "036000291452"],
        ["00001234", "00123456"],
        ["SO-00123", "{{lot}}"],
        ["SO-00123", "A|1_41"],
        ["A-00123", ",.^~_41"],
        ["SO-00123", "0000A^~_41"],
    ];
    let mut outputs = Vec::new();
    for lang in TemplateLanguage::ALL {
        for ((kind, format), values) in ObjectKind::BARCODES.into_iter().zip(formats).zip(pairs) {
            for dpmm in [8, 12] {
                let d = symbol(kind, values[0]);
                let template = load_template(lang, &d.to_print_template(lang, dpmm).unwrap());
                outputs.push((lang, kind, format, values, dpmm, template));
            }
        }
    }
    let mut positions = Vec::new();
    for lang in TemplateLanguage::ALL
        .into_iter()
        .filter(|l| *l != TemplateLanguage::Zpl)
    {
        for (&kind, values) in ObjectKind::BARCODES.iter().zip(pairs) {
            for angle in [0, 90, 180, 270] {
                for dpmm in [8, 12] {
                    let mut d = symbol(kind, values[0]);
                    d.height_mm = 100.;
                    d.objects[0].style.rotation = angle;
                    let Ok(template) = d.to_print_template(lang, dpmm) else {
                        continue;
                    };
                    let id = format!("position-{}-{kind:?}-{angle}-{dpmm}", lang.extension());
                    positions.push((id, lang, dpmm, d, template));
                }
            }
        }
    }
    assert_eq!(
        outputs.len(),
        TemplateLanguage::ALL.len() * ObjectKind::BARCODES.len() * 2
    );
    assert_eq!(positions.len(), 262);
    let mut errors = vec![];
    for (lang, kind, format, values, dpmm, template) in outputs {
        let base = format!("{lang:?}-{kind:?}-{dpmm}");
        for value in values {
            let bytes = template
                .fill(&[("value".into(), value.into())].into())
                .unwrap();
            let page = printed(&bytes, lang, dpmm, 100., 60.);
            let d = symbol(kind, value);
            let img = object_crop(&page, &d.objects[0], dpmm);
            let result = rxing::helpers::detect_in_luma(
                img.to_vec(),
                img.width(),
                img.height(),
                Some(format),
            );
            match result {
                Ok(decoded) if decoded.getText() == value => {}
                other => errors.push(format!(
                    "{base} {value}: {:?}",
                    other.map(|d| d.getText().to_owned())
                )),
            }
        }
    }
    for (id, lang, dpmm, d, source) in positions {
        let bytes = load_template(lang, &source).fill(&d.samples).unwrap();
        let page = printed(&bytes, lang, dpmm, d.width_mm, d.height_mm);
        let canvas = d.render_pixels(dpmm, RenderMode::Raster, 1, false).unwrap();
        let actual = ink_bounds(&page);
        let expected = ink_bounds(&canvas);
        // DPL's metric record positions round to tenths of a millimetre.
        let tolerance = u32::from(lang == TemplateLanguage::Dpl);
        if actual
            .iter()
            .zip(expected)
            .any(|(a, e)| a.abs_diff(e) > tolerance)
        {
            errors.push(format!("{id}: {actual:?} vs {expected:?}"));
        } else if d.objects[0].kind.is_matrix() {
            // Decode alone cannot tell if a square symbol was printed at the requested angle.
            let crop = |img: &GrayImage, [l, t, r, b]: [u32; 4]| {
                image::imageops::crop_imm(img, l, t, r - l + 1, b - t + 1).to_image()
            };
            if crop(&page, actual) != crop(&canvas, expected) {
                errors.push(format!("{id}: matrix pattern or orientation differs"));
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}

fn ink_bounds(image: &GrayImage) -> [u32; 4] {
    image
        .enumerate_pixels()
        .filter(|(_, _, p)| p[0] < 128)
        .fold([u32::MAX, u32::MAX, 0, 0], |[l, t, r, b], (x, y, _)| {
            [l.min(x), t.min(y), r.max(x), b.max(y)]
        })
}

#[test]
fn shipping_fields_and_retail_captions() {
    use labelize::elements::label_element::LabelElement;
    for lang in TemplateLanguage::ALL {
        check_print_order(lang);
        let mut design = shipping();
        let mut square = DesignObject::new(ObjectKind::Rectangle);
        square.x = 0.;
        square.y = 0.;
        square.width = 2.;
        square.height = 2.;
        square.style.filled = true;
        design.objects.insert(0, square);
        let bytes = design.to_print_template(lang, 8).unwrap();
        let job = fill_order(lang, &bytes, "000123", "{{order_no}} ^^~");
        assert_eq!(printed(&job, lang, 8, 100., 50.).get_pixel(5, 5)[0], 0);
        let mut retail = symbol(ObjectKind::Ean13, "012345678901");
        retail.objects[0].style.human_readable = true;
        let template = load_template(lang, &retail.to_print_template(lang, 8).unwrap());
        assert_eq!(template.fields()[0].uses.len(), 2);
        let fill = |s: &str| template.fill(&[("value".into(), s.into())].into());
        let job = fill("012345678901").unwrap();
        assert_eq!(job, fill("0123456789012").unwrap());
        assert!(fill("0123456789010").is_err());
        let labels = document::parse(&job, lang.document_kind(), &Settings::default()).unwrap();
        assert!(labels[0]
            .elements
            .iter()
            .any(|e| matches!(e, LabelElement::Text(t) if t.text == "0123456789012")));
    }
}

#[test]
fn binary_artwork_and_literal_braces_survive_filling() {
    use labelize::elements::label_element::LabelElement;
    for (lang, start, middle, end, codec) in [
        (
            TemplateLanguage::Tspl,
            "SIZE 40 mm,40 mm\r\nCLS\r\nBITMAP 0,0,10,1,0,",
            "\r\nTEXT 0,50,\"1\",0,1,1,\"",
            "\"\r\nPRINT 1,1\r\n",
            "tspl",
        ),
        (
            TemplateLanguage::Epl,
            "N\r\nq320\r\nQ320,0\r\nGW0,0,10,1,",
            "\r\nA0,50,0,1,1,1,N,\"",
            "\"\r\nP1\r\n",
            "epl",
        ),
        (
            TemplateLanguage::Cpcl,
            "! 0 200 200 320 1\r\nCG 10 1 0 0 ",
            "\r\nTEXT 0 0 0 50 ",
            "\r\nPRINT\r\n",
            "line",
        ),
    ] {
        let mut body = start.as_bytes().to_vec();
        body.extend_from_slice(b"\n\r\0\x02\xff{{x}}");
        body.extend_from_slice(middle.as_bytes());
        let offset = body.len();
        body.extend_from_slice(b"{{a}}");
        body.extend_from_slice(end.as_bytes());
        let directory = format!(
            "LABELIZE-TPL/1 {} US-ASCII\n{offset} 5 2 text {codec} -\n\n",
            lang.extension()
        );
        let template = load_template(lang, &[directory.as_bytes(), &body].concat());
        let job = template
            .fill(&[("a".into(), "{{a}}".into())].into())
            .unwrap();
        assert_eq!(job, body);
        let labels = document::parse(&job, lang.document_kind(), &Settings::default()).unwrap();
        assert!(labels[0]
            .elements
            .iter()
            .any(|e| matches!(e, LabelElement::GraphicField(_))));
        assert!(labels[0]
            .elements
            .iter()
            .any(|e| matches!(e, LabelElement::Text(t) if t.text=="{{a}}")));
    }
}

#[test]
fn escaped_and_counted_fields_decode_with_their_original_data() {
    use rxing::BarcodeFormat::{AZTEC, DATA_MATRIX, PDF_417, QR_CODE};
    use TemplateLanguage::{Cpcl, Epl, Tspl};
    for (lang, object, format, value) in [
        (Tspl, ObjectKind::Qr, QR_CODE, "A\\\"B\\"),
        (Epl, ObjectKind::Qr, QR_CODE, "A\"B\\C\\"),
        (Tspl, ObjectKind::Pdf417, PDF_417, ",L12,part\r\nPRINT"),
        (Tspl, ObjectKind::Aztec, AZTEC, "\"保温杯\r\nPRINT\""),
        (Cpcl, ObjectKind::DataMatrix, DATA_MATRIX, "eNdDaTaMaTrIx"),
        (
            Cpcl,
            ObjectKind::DataMatrix,
            DATA_MATRIX,
            "_d065_\r\n\u{001d}",
        ),
    ] {
        let mut d = symbol(object, "SO-00123");
        if format == AZTEC {
            d.set_printer_profile(
                lang,
                PrinterProfile {
                    font: "TSS24.BF2".into(),
                    encoding: TemplateEncoding::Utf8,
                },
            )
            .unwrap();
        }
        let template = load_template(lang, &d.to_print_template(lang, 8).unwrap());
        let bytes = template
            .fill(&[("value".into(), value.into())].into())
            .unwrap();
        let page = printed(&bytes, lang, 8, 100., 60.);
        let crop = object_crop(&page, &d.objects[0], 8);
        let decoded = rxing::helpers::detect_in_luma(
            crop.to_vec(),
            crop.width(),
            crop.height(),
            Some(format),
        )
        .unwrap();
        if format == AZTEC {
            let binary: Vec<_> = decoded
                .getText()
                .chars()
                .map(|c| u8::try_from(c).unwrap())
                .collect();
            assert_eq!(binary, value.as_bytes());
        } else {
            assert_eq!(decoded.getText(), value);
        }
    }
}

#[test]
fn template_previews_match_native_jobs_and_scan() {
    use document::{PreviewRequest, TemplateRecord, FILLED_JOB};
    use rxing::BarcodeFormat::{CODE_128, QR_CODE};
    let temp = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/print-templates");
    let cli = std::env::var_os("LABELIZE_CLI");
    for lang in TemplateLanguage::ALL {
        let name = if lang == TemplateLanguage::Dpl {
            "shipping-logo"
        } else {
            "shipping"
        };
        let design_file = examples().join(format!("{name}.labelize.json"));
        let design: DesignDocument =
            serde_json::from_slice(&fs::read(&design_file).unwrap()).unwrap();
        let ext = lang.extension();
        for dpmm in [8, 12] {
            let file = temp.path().join(format!("shipping.tpl.{ext}"));
            fs::write(&file, design.to_print_template(lang, dpmm).unwrap()).unwrap();
            assert_eq!(TemplateLanguage::from_template_path(&file), Some(lang));
            let settings = Settings {
                dpmm,
                width_mm: 100.,
                height_mm: 50.,
                ..Settings::default()
            };
            for record in ["first", "second"] {
                let data = examples().join(format!("{record}.json"));
                let values: TemplateValues =
                    serde_json::from_slice(&fs::read(&data).unwrap()).unwrap();
                let mut pages = Vec::new();
                let mut jobs = Vec::new();
                for (input, kind) in [
                    (&design_file, DocumentKind::Design),
                    (&file, lang.document_kind()),
                ] {
                    let request = PreviewRequest {
                        input: input.clone(),
                        kind,
                        settings: settings.clone(),
                        template: Some(TemplateRecord {
                            language: lang,
                            values: values.clone(),
                        }),
                    };
                    let dir = tempfile::tempdir_in(temp.path()).unwrap();
                    let result = document::preview(&request, dir.path()).unwrap();
                    assert_eq!(result.len(), 1);
                    jobs.push(fs::read(dir.path().join(FILLED_JOB)).unwrap());
                    pages.push(
                        image::open(dir.path().join(&result[0].file))
                            .unwrap()
                            .to_luma8(),
                    );
                }
                assert_eq!(jobs[0], jobs[1]);
                if dpmm == 8 {
                    assert_eq!(
                        jobs[0],
                        fs::read(fixtures.join(format!("{record}.{ext}"))).unwrap()
                    );
                }
                assert!(
                    pages[0] == pages[1],
                    "{record} {ext} {dpmm} preview origins"
                );
                assert!(pages[0] == printed(&jobs[0], lang, dpmm, 100., 50.));
                for (kind, format) in [(ObjectKind::Code128, CODE_128), (ObjectKind::Qr, QR_CODE)] {
                    let o = design.objects.iter().find(|o| o.kind == kind).unwrap();
                    let crop = object_crop(&pages[0], o, dpmm);
                    let result = rxing::helpers::detect_in_luma(
                        crop.to_vec(),
                        crop.width(),
                        crop.height(),
                        Some(format),
                    )
                    .unwrap();
                    assert_eq!(result.getText(), values["order_no"]);
                }
                if let Some(cli) = &cli {
                    let out = temp.path().join("cli.png");
                    let result = std::process::Command::new(cli)
                        .args(["template", "render"])
                        .arg(&file)
                        .arg("--data")
                        .arg(&data)
                        .args([
                            "--dpmm",
                            &dpmm.to_string(),
                            "--width",
                            "100",
                            "--height",
                            "50",
                            "-o",
                        ])
                        .arg(&out)
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    assert!(
                        pages[0] == image::open(out).unwrap().to_luma8(),
                        "{record} {ext} {dpmm} CLI pixels"
                    );
                }
            }
        }
    }
}

#[test]
fn parameter_errors_keep_the_schema_and_identify_each_object() {
    use document::{PreviewRequest, TemplateRecord, TemplateReport, FILLED_JOB, TEMPLATE_REPORT};
    let temp = tempfile::tempdir().unwrap();
    for lang in TemplateLanguage::ALL {
        let design = symbol(ObjectKind::Ean13, "0123456789012");
        let input = temp.path().join(format!("retail.tpl.{}", lang.extension()));
        fs::write(&input, design.to_print_template(lang, 8).unwrap()).unwrap();
        for values in [
            TemplateValues::new(),
            [("value".into(), "0123456789010".into())].into(),
        ] {
            let case = tempfile::tempdir_in(temp.path()).unwrap();
            let result = document::preview(
                &PreviewRequest {
                    input: input.clone(),
                    kind: lang.document_kind(),
                    settings: Settings::default(),
                    template: Some(TemplateRecord {
                        language: lang,
                        values,
                    }),
                },
                case.path(),
            )
            .unwrap();
            assert!(result.is_empty() && !case.path().join(FILLED_JOB).exists());
            let report: TemplateReport =
                serde_json::from_slice(&fs::read(case.path().join(TEMPLATE_REPORT)).unwrap())
                    .unwrap();
            assert_eq!(report.fields, ["value"]);
            assert!(report
                .problems
                .iter()
                .any(|p| p.fields == ["value"] && p.object == 1 && !p.message.is_empty()));
        }
    }
    let source = shipping()
        .to_print_template(TemplateLanguage::Epl, 8)
        .unwrap();
    let report = load_template(TemplateLanguage::Epl, &source).validate(
        &[
            ("order_no".into(), "000123".into()),
            ("product".into(), "保温杯".into()),
        ]
        .into(),
    );
    assert_eq!(report[0].fields, ["product"]);
    assert!(report[0].message.contains("ASCII"));
    assert!(TemplateLanguage::from_template_path(Path::new("shipping.tspl")).is_none());
    assert_eq!(
        TemplateLanguage::from_template_path(Path::new("Shipping.TPL.TsPl")),
        Some(TemplateLanguage::Tspl)
    );
}

#[test]
fn generated_slots_keep_printer_specific_validation_and_legacy_dpl_data() {
    for lang in [
        TemplateLanguage::Tspl,
        TemplateLanguage::Epl,
        TemplateLanguage::Cpcl,
        TemplateLanguage::Dpl,
    ] {
        for (kind, sample, invalid) in [
            (ObjectKind::Qr, "TEST", "abc".repeat(1000)),
            (ObjectKind::Pdf417, "TEST", "abc".repeat(1000)),
            (ObjectKind::Code39, "TEST", "A\\B".to_owned()),
        ] {
            let d = symbol(kind, sample);
            let template = load_template(lang, &d.to_print_template(lang, 8).unwrap());
            let values = [("value".into(), invalid)].into();
            let problems = template.validate(&values);
            assert!(
                problems.iter().any(|p| p.fields == ["value"]),
                "{lang:?} {kind:?}"
            );
            assert!(template.fill(&values).is_err());
        }
    }
    let lang = TemplateLanguage::Dpl;
    let d = symbol(ObjectKind::Aztec, "SO-00042");
    let bytes = d.to_print_template(lang, 8).unwrap();
    let current = load_template(lang, &bytes);
    let legacy = String::from_utf8(bytes)
        .unwrap()
        .replace("aztec dpl-aztec", "aztec line");
    let values = [("value".into(), "0000SO-00042".into())].into();
    let job = current.fill(&values).unwrap();
    assert!(job
        .windows(b"00000000SO-00042\r".len())
        .any(|bytes| bytes == b"00000000SO-00042\r"));
    assert_eq!(
        load_template(lang, legacy.as_bytes())
            .fill(&values)
            .unwrap(),
        job
    );
}
