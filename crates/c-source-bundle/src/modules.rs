use crate::{Result, c_source, read_text};
use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

#[derive(Clone, Debug, Serialize)]
pub struct Include {
    pub system: bool,
    pub name: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Serialize)]
pub struct Graph {
    pub sources: Vec<String>,
    pub generated: BTreeSet<String>,
    pub includes: BTreeMap<String, Vec<Include>>,
}

#[derive(Clone)]
pub struct Project {
    pub root: PathBuf,
    canonical_root: PathBuf,
    pub include_dirs: Vec<String>,
    pub generated: BTreeSet<String>,
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}

impl Project {
    pub fn new(root: &Path) -> Result<Self> {
        let root = std::path::absolute(root).map_err(|e| e.to_string())?;
        let root = normalize(&root);

        let canonical_root = root
            .canonicalize()
            .map_err(|e| format!("{}: {e}", root.display()))?;
        Ok(Self {
            root,
            canonical_root,
            include_dirs: vec!["src".into()],
            generated: BTreeSet::new(),
        })
    }

    pub fn source_path(&self, relative: &str) -> Result<PathBuf> {
        let relative = relative.replace('\\', "/");
        let path = normalize(&self.root.join(&relative));
        if !path.starts_with(self.root.clone()) {
            return Err(format!("Source outside project: {relative}"));
        }

        // Check the nearest existing ancestor too, so symlinks cannot escape the project.
        let existing = path
            .ancestors()
            .find(|p| p.exists())
            .ok_or_else(|| format!("Source outside project: {relative}"))?;
        let canonical = existing.canonicalize().map_err(|e| e.to_string())?;
        if !canonical.starts_with(&self.canonical_root) {
            return Err(format!("Source outside project: {relative}"));
        }

        Ok(path)
    }

    pub fn includes(&self, source: &str) -> Result<Vec<Include>> {
        static GUARD: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^#\s*ifndef\s+(\w+)\s*$").unwrap());
        static DEFINE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^#\s*define\s+(\w+)\s*$").unwrap());
        static DIRECTIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#\s*(\w+)").unwrap());
        static SYSTEM: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^#\s*include\s+<([^>]+)>\s*(?://.*)?$").unwrap());
        static LOCAL: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"^#\s*include\s+"([^"]+)"\s*(?://.*)?$"#).unwrap());

        let text = read_text(&self.source_path(source)?)?;
        let directives = c_source::directives(&text)?;
        let guarded = source.ends_with(".h")
            && directives.len() > 1
            && GUARD
                .captures(&directives[0].text)
                .zip(DEFINE.captures(&directives[1].text))
                .is_some_and(|(guard, define)| guard[1] == define[1]);

        let mut includes = Vec::new();
        let mut depth = 0_i32;

        for directive in directives {
            let line = directive.text.trim_end();
            let Some(keyword) = DIRECTIVE.captures(line) else {
                continue;
            };

            match &keyword[1] {
                "if" | "ifdef" | "ifndef" => depth += 1,
                "else" | "elif" if guarded && depth == 1 => {
                    return Err(format!("Unsupported header guard alternative in {source}"));
                }
                "endif" => depth -= 1,
                _ => {}
            }

            if &keyword[1] != "include" {
                continue;
            }

            if depth > i32::from(guarded) {
                return Err(format!("Conditional include is unsupported in {source}"));
            }

            let (system, name) = if let Some(found) = SYSTEM.captures(line) {
                (true, found[1].to_owned())
            } else if let Some(found) = LOCAL.captures(line) {
                let name = found[1].replace('\\', "/");
                let parent = Path::new(source).parent().unwrap();
                let candidates = std::iter::once(parent.join(&name)).chain(
                    self.include_dirs
                        .iter()
                        .map(|dir| Path::new(dir).join(&name)),
                );
                let mut resolved = None;

                for candidate in candidates {
                    let path = self.source_path(&candidate.to_string_lossy())?;
                    let logical = path
                        .strip_prefix(&self.root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    if path.is_file() || self.generated.contains(&logical) {
                        let relative = path
                            .strip_prefix(&self.root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        resolved = Some(relative);
                        break;
                    }
                }

                let resolved = resolved
                    .filter(|name| name.ends_with(".h") || name.ends_with(".inc"))
                    .ok_or_else(|| format!("Unknown local include in {source}: {name}"))?;
                (false, resolved)
            } else {
                return Err(format!("Unsupported include in {source}: {line}"));
            };

            includes.push(Include {
                system,
                name,
                start: directive.start,
                end: directive.end,
            });
        }

        Ok(includes)
    }

    pub fn resolve(&self, entries: &[String]) -> Result<Graph> {
        if entries.is_empty() {
            return Err("At least one C entry point is required".into());
        }
        let mut graph = Graph {
            sources: Vec::new(),
            generated: BTreeSet::new(),
            includes: BTreeMap::new(),
        };

        let mut seen = BTreeSet::new();
        for entry in entries {
            if !entry.ends_with(".c") || !self.source_path(entry)?.is_file() {
                return Err(format!("Unknown C entry point: {entry}"));
            }
            self.visit(entry, &mut graph, &mut seen)?;
        }

        Ok(graph)
    }

    fn visit(&self, source: &str, graph: &mut Graph, seen: &mut BTreeSet<String>) -> Result<()> {
        if !seen.insert(source.to_owned()) {
            return Ok(());
        }
        if source.ends_with(".c") {
            graph.sources.push(source.to_owned());
        }

        let includes = self.includes(source)?;
        graph.includes.insert(source.to_owned(), includes.clone());

        for include in includes {
            if include.system {
                continue;
            }

            if self.generated.contains(&include.name) {
                graph.generated.insert(include.name);
                continue;
            }

            self.visit(&include.name, graph, seen)?;
            let implementation = Path::new(&include.name)
                .with_extension("c")
                .to_string_lossy()
                .replace('\\', "/");
            if self.source_path(&implementation)?.is_file() {
                self.visit(&implementation, graph, seen)?;
            }
        }

        Ok(())
    }
}

pub fn module_names(sources: &[String]) -> Result<BTreeMap<String, String>> {
    let mut sorted = sources.to_vec();
    sorted.sort();

    let mut used = BTreeMap::new();
    let mut names = BTreeMap::new();

    for source in sorted {
        let stem = source.trim_start_matches("src/").trim_end_matches(".c");
        let name: String = stem
            .replace('/', "__")
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        if let Some(previous) = used.insert(name.clone(), source.clone()) {
            return Err(format!(
                "Module name collision: {previous} and {source} both normalize to {name}"
            ));
        }

        names.insert(source, name);
    }

    Ok(names)
}
