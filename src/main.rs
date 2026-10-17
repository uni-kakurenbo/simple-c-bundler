use c_source_tools::{Build, Result, Settings, read_text, templates};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "c-source-tools bundle|resolve --root DIR --entry FILE... [--output FILE] [--config FILE] [--compiler PROGRAM]\nc-source-tools generate --root DIR [--config FILE] [--compiler PROGRAM]\nc-source-tools template FILE\n";

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or(HELP)?;
    if action == "--version" {
        println!("c-source-tools {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if matches!(action.as_str(), "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if action == "template" {
        let file = args.next().ok_or("Expected template FILE")?;
        if args.next().is_some() {
            return Err("Unexpected template argument".into());
        }
        print!("{}", templates::convert(&read_text(Path::new(&file))?)?);
        return Ok(());
    }
    if !matches!(action.as_str(), "bundle" | "resolve" | "generate") {
        return Err(format!("Unknown command: {action}\n{HELP}"));
    }
    let mut root = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut config = None;
    let mut output = None;
    let mut compiler = None;
    let mut entries = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(option) = args.next() {
        if option != "--entry" && !seen.insert(option.clone()) {
            return Err(format!("Duplicate option: {option}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {option}"))?;
        match option.as_str() {
            "--root" => root = value.into(),
            "--config" => config = Some(value),
            "--output" => output = Some(PathBuf::from(value)),
            "--compiler" => compiler = Some(value),
            "--entry" => entries.push(value),
            _ => return Err(format!("Unknown option: {option}")),
        }
    }
    let config = config
        .map(|path| root.join(path))
        .unwrap_or_else(|| root.join("c-source-tools.json"));
    let mut settings: Settings = if config.is_file() {
        serde_json::from_str(&read_text(&config)?)
            .map_err(|e| format!("{}: {e}", config.display()))?
    } else if seen.contains("--config") {
        return Err(format!("Configuration not found: {}", config.display()));
    } else {
        Settings::default()
    };
    if let Some(program) = compiler {
        for assembly in &mut settings.assemblies {
            assembly.compiler.program.clone_from(&program);
        }
    }
    let build = Build::with_settings(&root, settings)?;
    let result = match action.as_str() {
        "bundle" => {
            serde_json::to_value(build.bundle(&entries, &output.ok_or("Missing --output FILE")?)?)
        }
        "resolve" => serde_json::to_value(build.resolve(&entries)?),
        "generate" => serde_json::to_value(build.generate()?),
        _ => unreachable!(),
    }
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
