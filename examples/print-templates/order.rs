use labelize::{render_png, Language, PrintTemplate, RenderOptions, TemplateValues};
use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [source, order, output] = args.as_slice() else {
        return Err("Usage: preview-order TEMPLATE ORDER.json OUTPUT_DIRECTORY".into());
    };
    let source = Path::new(source);
    let extension = source
        .extension()
        .and_then(|s| s.to_str())
        .ok_or("Missing printer language")?;
    let language = Language::from_name(extension).ok_or("Unknown printer language")?;
    let template = PrintTemplate::load(language, &fs::read(source)?)?;
    let values: TemplateValues = serde_json::from_slice(&fs::read(order)?)?;
    let job = template.fill(&values)?;
    let options = RenderOptions {
        language: Some(language),
        dpmm: 8,
        width_mm: Some(100.),
        height_mm: Some(50.),
        ..Default::default()
    };
    let preview = render_png(&job, options)?;
    let output = Path::new(output);
    fs::create_dir_all(output)?;
    fs::write(output.join(format!("order.{extension}")), job)?;
    fs::write(output.join("order.png"), preview.bytes)?;
    Ok(())
}
