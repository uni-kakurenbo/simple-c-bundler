use crate::c_source::{Scanner, directives, lex};
use crate::modules::{Graph, Project, module_names};
use crate::{Result, read_text, templates, write_text};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;

/// No filesystem writes occur until all templates and C bodies are validated.
pub struct Rendered {
    pub text: String,
    pub generated: BTreeMap<String, String>,
    pub graph: Graph,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleOutput {
    pub bundle: String,
    pub generated_root: String,
    pub sources: Vec<String>,
    pub templates: BTreeSet<String>,
}

struct Expander<'a> {
    project: &'a Project,
    graph: &'a Graph,
    generated: &'a BTreeMap<String, String>,
    headers: String,
    seen: BTreeSet<String>,
    system_headers: BTreeSet<String>,
}

fn strip_pragma_once(text: &str) -> Result<String> {
    let mut stripped = String::new();
    let mut cursor = 0;

    for directive in directives(text)? {
        let logical = directive.text.replace("\\\n", "");
        let tokens = lex(&logical, false)?;
        if !tokens
            .iter()
            .map(|token| token.text.as_str())
            .eq(["#", "pragma", "once"])
        {
            continue;
        }

        let line_start = text[..directive.start]
            .rfind('\n')
            .map_or(0, |offset| offset + 1);
        let start = if text[line_start..directive.start].trim().is_empty() {
            line_start
        } else {
            directive.start
        };

        stripped.push_str(&text[cursor..start]);
        cursor = directive.end;
    }

    stripped.push_str(&text[cursor..]);

    Ok(stripped)
}

impl Expander<'_> {
    fn expand(&mut self, source: &str) -> Result<String> {
        let content = read_text(&self.project.source_path(source)?)?;
        let mut expanded = String::new();
        let mut cursor = 0;

        for include in &self.graph.includes[source] {
            expanded.push_str(&content[cursor..include.start]);
            cursor = include.end;

            if include.system {
                self.system_headers.insert(include.name.clone());
            } else if let Some(generated) = self.generated.get(&include.name) {
                let template = Path::new(&include.name)
                    .with_extension("asm")
                    .to_string_lossy()
                    .replace('\\', "/");

                writeln!(expanded, "/* Generated from {template} */").unwrap();
                expanded.push_str(generated);
            } else if self.seen.insert(include.name.clone()) {
                let header = self.expand(&include.name)?;
                writeln!(self.headers, "\n/* ===== {} ===== */", include.name).unwrap();
                writeln!(self.headers, "{header}").unwrap();
            }
        }

        expanded.push_str(&content[cursor..]);

        strip_pragma_once(&expanded).map_err(|error| format!("Could not expand {source}: {error}"))
    }
}

pub fn render(project: &Project, profile: &str) -> Result<Rendered> {
    let graph = project.resolve(profile)?;
    let module_names = module_names(&graph.sources)?;

    let mut generated = BTreeMap::new();
    for include in &graph.templates {
        let template = Path::new(include).with_extension("asm");
        let template = template.to_string_lossy();
        let code = templates::convert(&read_text(&project.source_path(&template)?)?)
            .map_err(|e| format!("{template}: {e}"))?;
        generated.insert(include.clone(), code);
    }

    let mut expander = Expander {
        project,
        graph: &graph,
        generated: &generated,
        headers: String::new(),
        seen: BTreeSet::new(),
        system_headers: BTreeSet::new(),
    };

    let mut bodies = String::new();

    for source in &graph.sources {
        let body = expander.expand(source)?;
        let name = &module_names[source];
        writeln!(bodies, "\n/* ===== Module: {name} =====").unwrap();
        writeln!(bodies, " * Source: {source}").unwrap();

        let scanner =
            Scanner::new(&body).map_err(|e| format!("Could not qualify {source}: {e}"))?;
        let locals = scanner.private_names();
        if !locals.is_empty() {
            bodies.push_str(" * Identifier mapping:\n");
            for local in locals {
                writeln!(bodies, " *   {local} -> {name}__{local}").unwrap();
            }
        }

        bodies.push_str(" */\n");
        writeln!(bodies, "{}", scanner.rewrite(&body, &format!("{name}__"))?).unwrap();
    }

    let mut text = format!("/* Generated from src/ by simple-c-bundler (profile: {profile}). */\n");
    for header in &expander.system_headers {
        writeln!(text, "#include <{header}>").unwrap();
    }

    text.push_str(&expander.headers);
    text.push_str(&bodies);

    Ok(Rendered {
        text,
        generated,
        graph,
    })
}

pub fn export(root: &Path, profile: &str) -> Result<BundleOutput> {
    let project = Project::new(root)?;
    let rendered = render(&project, profile)?;

    let generated_root = project.root.join("build/generated").join(profile);

    for (include, text) in &rendered.generated {
        write_text(
            &generated_root.join(include.strip_prefix("src/").unwrap()),
            text,
        )?;
    }

    let bundle = project.root.join("dist").join(format!("{profile}.c"));
    write_text(&bundle, &rendered.text)?;

    Ok(BundleOutput {
        bundle: bundle.to_string_lossy().into_owned(),
        generated_root: generated_root.to_string_lossy().into_owned(),
        sources: rendered.graph.sources,
        templates: rendered.graph.templates,
    })
}
