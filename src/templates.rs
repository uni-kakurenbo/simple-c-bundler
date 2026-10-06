use crate::Result;
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
enum Type {
    Int,
    Uint,
    Size,
    String,
}

impl Type {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "int" => Some(Self::Int),
            "uint" => Some(Self::Uint),
            "size" => Some(Self::Size),
            "string" => Some(Self::String),
            _ => None,
        }
    }

    fn c_type(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Uint => "unsigned",
            Self::Size => "size_t",
            Self::String => "const char *",
        }
    }

    fn format(self) -> &'static str {
        match self {
            Self::Int => "%d",
            Self::Uint => "%u",
            Self::Size => "%zu",
            Self::String => "%s",
        }
    }
}

struct Block<'a> {
    name: &'a str,
    parameters: Vec<(&'a str, Type)>,
    lines: Vec<&'a str>,
}

fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn substitute(line: &str, block: &Block<'_>, arguments: &mut Vec<String>) -> Result<String> {
    let mut output = String::new();
    let mut rest = line;

    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let placeholder = &rest[start + 2..];
        let end = placeholder.find("}}").ok_or("Malformed placeholder")?;
        let name = &placeholder[..end];
        if !identifier(name) {
            return Err("Malformed placeholder".into());
        }

        let (_, kind) = block
            .parameters
            .iter()
            .find(|(parameter, _)| *parameter == name)
            .ok_or_else(|| format!("Unknown placeholder: {name}"))?;

        arguments.push(name.to_owned());
        output.push_str(kind.format());
        rest = &placeholder[end + 2..];
    }

    output.push_str(rest);
    if output.contains("{{") || output.contains("}}") {
        return Err("Malformed placeholder".into());
    }

    Ok(output)
}

pub fn convert(text: &str) -> Result<String> {
    let mut blocks: Vec<Block<'_>> = Vec::new();
    let mut names = BTreeSet::new();

    for line in text.lines() {
        if let Some(declaration) = line.strip_prefix('@') {
            let parts: Vec<_> = declaration
                .split(' ')
                .filter(|part| !part.is_empty())
                .collect();
            let Some(&name) = parts.first() else {
                return Err(format!("Invalid template: {line}"));
            };
            if !identifier(name) || !names.insert(name) {
                return Err(format!("Invalid or duplicate template: {line}"));
            }

            let mut parameters = Vec::new();
            for part in &parts[1..] {
                let (parameter, kind) = part
                    .split_once(':')
                    .ok_or_else(|| format!("Invalid template parameter: {part}"))?;
                let kind = Type::parse(kind)
                    .filter(|_| identifier(parameter))
                    .ok_or_else(|| format!("Invalid template parameter: {part}"))?;
                if parameter == "output" || parameters.iter().any(|(name, _)| *name == parameter) {
                    return Err(format!("Duplicate/reserved parameter: {parameter}"));
                }
                parameters.push((parameter, kind));
            }

            blocks.push(Block {
                name,
                parameters,
                lines: Vec::new(),
            });
        } else if let Some(block) = blocks.last_mut() {
            block.lines.push(line);
        } else {
            return Err("Assembly before first template".into());
        }
    }

    if blocks.is_empty() {
        return Err("No templates".into());
    }

    let mut code = String::new();
    for block in blocks {
        if block.lines.is_empty() {
            return Err(format!("Empty template: {}", block.name));
        }

        let mut signature = vec!["FILE *output".to_owned()];
        signature.extend(
            block
                .parameters
                .iter()
                .map(|(name, kind)| format!("{} {name}", kind.c_type())),
        );

        writeln!(
            code,
            "static void asm_{}({})",
            block.name,
            signature.join(", ")
        )
        .unwrap();
        code.push_str("{\n");

        let formatted = !block.parameters.is_empty();
        let mut arguments = Vec::new();
        let mut literals = Vec::new();

        for line in &block.lines {
            let line = if formatted {
                line.replace('%', "%%")
            } else {
                (*line).to_owned()
            };

            let line = substitute(&line, &block, &mut arguments)?;
            let escaped = line
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\t', "\\t");
            literals.push(format!("        \"{escaped}\\n\""));
        }

        for (name, _) in &block.parameters {
            if !arguments.iter().any(|argument| argument == name) {
                return Err(format!("Unused template parameter: {name}"));
            }
        }

        if formatted {
            code.push_str("    fprintf(output,\n");
            writeln!(code, "{}, {});", literals.join("\n"), arguments.join(", ")).unwrap();
        } else {
            code.push_str("    fputs(\n");
            writeln!(code, "{}, output);", literals.join("\n")).unwrap();
        }

        code.push_str("}\n\n");
    }

    Ok(code)
}
