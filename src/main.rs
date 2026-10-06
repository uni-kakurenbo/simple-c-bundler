use simple_c_bundler::{Result, bundle, modules::Project, read_text, templates};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const HELP: &str = "\
simple-c-bundler bundle|resolve --root DIRECTORY --profile NAME
simple-c-bundler template FILE

bundle   Write dist/NAME.c and build/generated/NAME/*.inc; print a JSON build manifest.
resolve  Print the source dependency graph as JSON without writing files.
template Convert an assembly template to C on stdout without writing files.
";

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let action = args.next().ok_or(HELP)?;

    if matches!(action.as_str(), "--help" | "-h") {
        print!("{HELP}");
        return Ok(());
    }

    if action == "template" {
        let file = args.next().ok_or("Expected template FILE")?;
        if args.next().is_some() {
            return Err("Unexpected template argument".into());
        }

        let text = templates::convert(&read_text(Path::new(&file))?)?;
        print!("{text}");
        return Ok(());
    }

    if !matches!(action.as_str(), "bundle" | "resolve") {
        return Err(format!("Unknown command: {action}\n{HELP}"));
    }

    let mut root = None;
    let mut profile = None;

    while let Some(option) = args.next() {
        let target = match option.as_str() {
            "--root" if root.is_none() => &mut root,
            "--profile" if profile.is_none() => &mut profile,
            _ => return Err(format!("Unknown or duplicate option: {option}")),
        };

        *target = Some(
            args.next()
                .ok_or_else(|| format!("Missing value for {option}"))?,
        );
    }

    let root = PathBuf::from(root.ok_or("Missing --root DIRECTORY")?);
    let profile = profile.ok_or("Missing --profile NAME")?;

    let json = if action == "bundle" {
        serde_json::to_string(&bundle::export(&root, &profile)?)
    } else {
        serde_json::to_string(&Project::new(&root)?.resolve(&profile)?)
    }
    .map_err(|e| e.to_string())?;

    println!("{json}");

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
