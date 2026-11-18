#![cfg(feature = "render")]

use image::GrayImage;
use labelize_printer_core::{
    design::{
        Background, DesignDocument, DesignObject, HorizontalAlign, ObjectKind, VerticalAlign,
    },
    document::{self, DocumentKind},
    RenderMode, Settings,
};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const SHIPPING: &str = include_str!("../../../examples/java/shipping.labelize.json");

fn shipping() -> DesignDocument {
    serde_json::from_str(SHIPPING).unwrap()
}

fn object(kind: ObjectKind, sample: &str) -> DesignDocument {
    let mut o = DesignObject::new(kind);
    o.text = "{{value}}".into();
    o.variable = true;
    o.x = 4.;
    o.y = 4.;
    o.width = if kind.is_matrix() && kind != ObjectKind::Pdf417 {
        35.
    } else {
        80.
    };
    o.height = 35.;
    o.style.module_mm = 0.375;
    if kind.is_matrix() {
        o.style.horizontal = HorizontalAlign::Center;
        o.style.vertical = VerticalAlign::Middle;
    }
    DesignDocument {
        version: 3,
        objects: vec![o],
        samples: [("value".into(), sample.into())].into(),
        ..Default::default()
    }
}

fn rendered(zpl: &[u8], dpmm: i32) -> GrayImage {
    let settings = Settings {
        dpmm,
        output_dir: std::env::temp_dir(),
        ..Default::default()
    };
    let mut page = None;
    document::render(zpl, DocumentKind::Zpl, &settings, |p| {
        assert!(page.is_none(), "one record must produce one label");
        page = Some(p.pixels);
        Ok(())
    })
    .unwrap();
    page.unwrap()
}

#[test]
fn bindings_are_opt_in_shared_and_portable() {
    let mut d = DesignDocument::default();
    d.objects[0].text = "{{value}}".into();
    assert_eq!(d.portable_version(), 2);
    assert!(d.validate(8).is_ok());
    let old_pixels = d.render_pixels(8, RenderMode::Raster, 1, false).unwrap();
    d.objects.push(d.objects[0].clone());
    d.set_variable(0, true).unwrap();
    d.set_variable(1, true).unwrap();
    assert_eq!(d.version, 3);
    assert_eq!(d.samples.len(), 1);
    d.samples.insert("value".into(), "SO-00123".into());
    assert_ne!(
        old_pixels,
        d.render_pixels(8, RenderMode::Raster, 1, false).unwrap()
    );
    let reopened: DesignDocument =
        serde_json::from_slice(&serde_json::to_vec(&d).unwrap()).unwrap();
    assert_eq!(d, reopened);
    let zpl = d.to_zpl_template(8).unwrap();
    assert_eq!(zpl.matches("{{value}}").count(), 2);
    assert!(!zpl.contains("SO-00123"));
    let sample_pixels = rendered(d.to_zpl(8).unwrap().as_bytes(), 8);
    for i in [0, 1] {
        d.set_variable(i, false).unwrap();
    }
    assert_eq!(d.version, 2);
    assert!(d.samples.is_empty());
    assert_eq!(
        sample_pixels,
        d.render_pixels(8, RenderMode::Raster, 1, false).unwrap()
    );
    assert!(!reopened.vector_pdf(8).unwrap().is_empty());
    let mut wrong = reopened;
    wrong.version = 2;
    assert!(wrong.validate(8).is_err());
    wrong.version = 3;
    wrong.objects[1].text = "{{bad name}}".into();
    assert!(wrong.validate(8).is_err());
}

#[test]
fn exported_example_and_layer_rules() {
    let d = shipping();
    assert_eq!(
        d.to_zpl_template(8).unwrap(),
        include_str!("../../../examples/java/shipping.tpl.zpl")
    );
    let mut d = object(ObjectKind::Text, "中文");
    assert!(format!("{:#}", d.to_zpl_template(8).unwrap_err()).contains("font path"));
    d.printer_font = "E:MYFONT.TTF".into();
    let zpl = d.to_zpl_template(8).unwrap();
    assert!(zpl.contains("^CI28") && zpl.contains("^A@N,32,32,E:MYFONT.TTF"));
    d.printer_font = "E:x^FS".into();
    assert!(d.to_zpl_template(8).is_err());
    let mut d = object(ObjectKind::Text, "Sample");
    let mut rect = DesignObject::new(ObjectKind::Rectangle);
    rect.style.filled = true;
    rect.style.background = Background::White;
    rect.style.name = "foreground".into();
    d.objects.push(rect);
    let error = format!("{:#}", d.to_zpl_template(8).unwrap_err());
    assert!(error.contains("foreground") && error.contains("Object 1"));
    d.objects.swap(0, 1);
    assert!(d.to_zpl_template(8).unwrap().contains("^GFA"));
    d.objects[0].style.visible = false;
    assert!(!d.to_zpl_template(8).unwrap().contains("^GFA"));
    d.objects[1].style.background = Background::Black;
    let reverse = d.to_zpl_template(8).unwrap().replace("{{value}}", "Sample");
    let page = rendered(reverse.as_bytes(), 8);
    let ink = image::imageops::crop_imm(&page, 32, 32, 640, 280).to_image();
    assert!(ink.pixels().any(|p| p[0] == 0) && ink.pixels().any(|p| p[0] == 255));
}

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
#[ignore = "end-to-end interoperability requires JDK 17 or newer"]
fn java_fills_exported_zpl_and_independent_scanners_read_all_ten_codes() {
    use rxing::BarcodeFormat::*;
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/java");
    let probe = dir.join("Fill.java");
    fs::write(
        &probe,
        r#"import java.nio.file.*;
import java.util.Map;
public class Fill {
    public static void main(String[] a) throws Exception {
        System.out.write(ZplTemplate.fill(Files.readString(Path.of(a[0])),
            a.length == 2 ? Map.of("value", a[1]) : Map.of()));
    }
}"#,
    )
    .unwrap();
    success(
        Command::new("javac")
            .args(["-encoding", "UTF-8", "-d"])
            .arg(dir)
            .arg(examples.join("ZplTemplate.java"))
            .arg(examples.join("OrderLabel.java"))
            .arg(probe)
            .output()
            .unwrap(),
    );
    let template = dir.join("label.tpl.zpl");
    let fill = |value: Option<&str>| {
        let mut command = Command::new("java");
        command.arg("-cp").arg(dir).arg("Fill").arg(&template);
        if let Some(value) = value {
            command.arg(value);
        }
        command.output().unwrap()
    };
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
        ["SO-00123", "A>8^~_"],
        ["A-0001", "Z/002"],
        ["0123456789012", "5901234123457"],
        ["01234565", "96385074"],
        ["012345678905", "036000291452"],
        ["00001234", "00123456"],
        ["SO-00123", "标^~_{{x}}"],
        ["SO-00123", "A|1_41"],
        ["A-00123", "A^~_41"],
        ["SO-00123", "A^~_41"],
    ];
    for ((kind, format), values) in ObjectKind::BARCODES.into_iter().zip(formats).zip(pairs) {
        for dpmm in [8, 12] {
            let d = object(kind, values[0]);
            fs::write(&template, d.to_zpl_template(dpmm).unwrap()).unwrap();
            for value in values {
                let bytes = success(fill(Some(value)));
                let page = rendered(&bytes, dpmm);
                let o = &d.objects[0];
                let dot = |mm: f64| (mm * f64::from(dpmm)).round() as u32;
                let crop = image::imageops::crop_imm(
                    &page,
                    dot(o.x),
                    dot(o.y),
                    dot(o.width),
                    dot(o.height),
                )
                .to_image();
                let decoded = rxing::helpers::detect_in_luma(
                    crop.to_vec(),
                    crop.width(),
                    crop.height(),
                    Some(format),
                )
                .unwrap_or_else(|e| panic!("{kind:?}, {dpmm} dpmm, {value}: {e:?}"));
                assert_eq!(decoded.getText(), value, "{kind:?}, {dpmm} dpmm");
            }
        }
    }
    fs::write(
        &template,
        object(ObjectKind::Ean13, "5901234123457")
            .to_zpl_template(8)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        success(fill(Some("590123412345"))),
        success(fill(Some("5901234123457")))
    );
    for bad in [None, Some("000ABC"), Some("5901234123450")] {
        let result = fill(bad);
        assert!(!result.status.success() && result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("value"));
    }
    // Run the bundled business application against the exported classpath resource.
    fs::write(
        dir.join("shipping.tpl.zpl"),
        shipping().to_zpl_template(8).unwrap(),
    )
    .unwrap();
    for order in ["SO-00123", "SO-00456"] {
        let path = dir.join("order.zpl");
        success(
            Command::new("java")
                .arg("-cp")
                .arg(dir)
                .arg("OrderLabel")
                .args([order, "A^~_{{order_no}}"])
                .arg(&path)
                .output()
                .unwrap(),
        );
        let result = fs::read_to_string(path).unwrap();
        assert_eq!(result.matches(order).count(), 3);
        assert!(result.contains("A_5E_7E_5F_7B_7Border_5Fno_7D_7D"));
        assert_eq!(rendered(result.as_bytes(), 8).dimensions(), (800, 400));
        let settings = Settings {
            output_dir: dir.to_owned(),
            ..Default::default()
        };
        let (pdf, _) =
            document::vector_pdf(result.as_bytes(), DocumentKind::Zpl, &settings).unwrap();
        let visible = lopdf::Document::load_mem(&pdf)
            .unwrap()
            .extract_text(&[1])
            .unwrap();
        let visible = visible.replace('\n', "");
        assert!(
            visible.contains(order) && visible.contains("A^~_{{order_no}}"),
            "{visible:?}"
        );
    }
    let mut chinese = object(ObjectKind::Text, "订单");
    chinese.printer_font = "E:CJK.TTF".into();
    fs::write(&template, chinese.to_zpl_template(8).unwrap()).unwrap();
    let bytes = String::from_utf8(success(fill(Some("订单^~_{{value}}")))).unwrap();
    assert!(bytes.contains("^A@N,32,32,E:CJK.TTF"));
    assert!(bytes.contains("_E8_AE_A2_E5_8D_95_5E_7E_5F_7B_7Bvalue_7D_7D"));
}

#[test]
fn native_symbols_use_the_designs_dot_coordinates() {
    let bounds = |image: &GrayImage| {
        image
            .enumerate_pixels()
            .filter(|(_, _, p)| p[0] == 0)
            .fold((u32::MAX, u32::MAX, 0, 0), |(l, t, r, b), (x, y, _)| {
                (l.min(x), t.min(y), r.max(x), b.max(y))
            })
    };
    for kind in [
        ObjectKind::Code128,
        ObjectKind::Qr,
        ObjectKind::DataMatrix,
        ObjectKind::Aztec,
        ObjectKind::Pdf417,
    ] {
        let mut design = object(kind, "SO-00123");
        for angle in [0, 90, 180, 270] {
            if kind == ObjectKind::Qr && angle != 0 {
                design.objects[0].style.rotation = angle;
                assert!(
                    format!("{:#}", design.to_zpl_template(8).unwrap_err()).contains("rotation")
                );
                continue;
            }
            design.objects[0].style.rotation = angle;
            let job = design
                .to_zpl_template(8)
                .unwrap()
                .replace("{{value}}", "SO-00123");
            let output = rendered(job.as_bytes(), 8);
            let canvas = design
                .render_pixels(8, RenderMode::Raster, 1, false)
                .unwrap();
            assert_eq!(
                bounds(&output),
                bounds(&canvas),
                "{kind:?}, {angle} degrees"
            );
        }
    }
}
