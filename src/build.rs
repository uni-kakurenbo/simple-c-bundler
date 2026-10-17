use crate::compiler::Assembly;
use crate::{Result, read_text, templates, write_text};
use c_source_bundle::bundle::{self, Rendered};
use c_source_bundle::modules::{Graph, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Include paths, template directories, generated header output, and C assembly recipes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub include_dirs: Vec<String>,
    pub template_dirs: Vec<String>,
    pub generated_root: String,
    pub assemblies: Vec<Assembly>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            include_dirs: vec!["src".into()],
            template_dirs: vec!["src".into()],
            generated_root: "build/generated/include".into(),
            assemblies: Vec::new(),
        }
    }
}

/// Generate project assets and combine reachable C modules into a standalone source.
pub struct Build {
    pub root: PathBuf,
    pub settings: Settings,
    /// Environment passed to compiler children and excluded from cache keys.
    /// Put variables that affect generated output in `Compiler::environment`.
    pub environment: BTreeMap<OsString, OsString>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedOutput {
    pub generated_root: String,
    pub templates: Vec<String>,
    pub generated: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleOutput {
    pub bundle: String,
    pub generated_root: String,
    pub sources: Vec<String>,
    pub templates: BTreeSet<String>,
    pub generated: BTreeSet<String>,
}

fn files(directory: &Path, extension: &str, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        fs::read_dir(directory).map_err(|error| format!("{}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_dir() {
            files(&path, extension, output)?;
        } else if kind.is_file() && path.extension().is_some_and(|value| value == extension) {
            output.push(path);
        }
    }
    Ok(())
}

impl Build {
    /// Use the default include paths and output directory, with no assembly recipes.
    pub fn new(root: &Path) -> Result<Self> {
        Self::with_settings(root, Settings::default())
    }

    /// Validate and use explicit generation settings for the project at `root`.
    pub fn with_settings(root: &Path, mut settings: Settings) -> Result<Self> {
        let project = Project::new(root)?;
        let generated = project.source_path(&settings.generated_root)?;
        if settings
            .include_dirs
            .iter()
            .any(|dir| project.root.join(dir).starts_with(&generated))
        {
            return Err("Generated headers must not replace an input include directory".into());
        }
        let logical = |name: &str| -> Result<String> {
            Ok(project
                .source_path(name)?
                .strip_prefix(&project.root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"))
        };
        let mut outputs = BTreeSet::new();
        for assembly in &mut settings.assemblies {
            assembly.source = logical(&assembly.source)?;
            assembly.include = logical(&assembly.include)?;
            assembly.output = logical(&assembly.output)?;
            crate::literal::function(&assembly.function, &[])?;
            if !outputs.insert(assembly.output.clone()) {
                return Err(format!("Duplicate assembly output: {}", assembly.output));
            }
        }
        Ok(Self {
            root: project.root,
            settings,
            environment: BTreeMap::new(),
        })
    }

    pub fn source_path(&self, name: &str) -> Result<PathBuf> {
        Project::new(&self.root)?.source_path(name)
    }

    fn inputs(&self) -> Result<(Project, BTreeMap<String, String>)> {
        let mut project = Project::new(&self.root)?;
        project.include_dirs.clone_from(&self.settings.include_dirs);
        let mut paths = Vec::new();
        for dir in &self.settings.template_dirs {
            files(&project.source_path(dir)?, "asm", &mut paths)?;
        }
        paths.sort();
        let mut sources = BTreeMap::new();
        for path in paths {
            let source = path
                .strip_prefix(&self.root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let include = Path::new(&source)
                .with_extension("inc")
                .to_string_lossy()
                .replace('\\', "/");
            project.generated.insert(include.clone());
            sources.insert(include, source);
        }
        for assembly in &self.settings.assemblies {
            project.source_path(&assembly.include)?;
            if !project.generated.insert(assembly.include.clone()) {
                return Err(format!("Duplicate generated include: {}", assembly.include));
            }
        }
        Ok((project, sources))
    }

    /// Resolve entry sources into a dependency graph without generating files.
    pub fn resolve(&self, entries: &[String]) -> Result<Graph> {
        self.inputs()?.0.resolve(entries)
    }

    fn prepare(
        &self,
        selected: Option<&BTreeSet<String>>,
    ) -> Result<(Project, BTreeMap<String, String>)> {
        let (project, templates) = self.inputs()?;
        let needed = |include: &String| selected.is_none_or(|selected| selected.contains(include));
        let mut generated = BTreeMap::new();
        // Validate every selected template before invoking any compiler or publishing headers.
        for (include, source) in templates {
            if needed(&include) {
                generated.insert(
                    include,
                    templates::convert(&read_text(&project.source_path(&source)?)?)
                        .map_err(|error| format!("{source}: {error}"))?,
                );
            }
        }
        for assembly in &self.settings.assemblies {
            if needed(&assembly.include) {
                generated.insert(
                    assembly.include.clone(),
                    assembly.generate(&project, &self.environment)?,
                );
            }
        }
        Ok((project, generated))
    }

    /// Prepare reachable assets and return the rendered bundle in memory.
    pub fn render(&self, entries: &[String]) -> Result<Rendered> {
        let graph = self.resolve(entries)?;
        let (project, generated) = self.prepare(Some(&graph.generated))?;
        bundle::render(&project, entries, &generated)
    }

    fn publish(&self, generated: &BTreeMap<String, String>) -> Result<PathBuf> {
        let root = self.source_path(&self.settings.generated_root)?;
        for (include, code) in generated {
            let logical = self.source_path(include)?;
            let relative = logical.strip_prefix(&self.root).unwrap();
            let destination = root.join(relative);
            // Check existing ancestors too, before following a generated-directory symlink.
            self.source_path(&destination.to_string_lossy())?;
            write_text(&destination, code)?;
        }
        Ok(root)
    }

    /// Generate all assets and remove stale `.h` and `.inc` files under the generated root.
    pub fn generate(&self) -> Result<GeneratedOutput> {
        let (_, generated) = self.prepare(None)?;
        let root = self.publish(&generated)?;
        // Remove stale .h and .inc outputs from the dedicated generated include directory.
        let expected: BTreeSet<_> = generated.keys().map(|name| root.join(name)).collect();
        for extension in ["inc", "h"] {
            let mut existing = Vec::new();
            if root.is_dir() {
                files(&root, extension, &mut existing)?;
            }
            for path in existing {
                if !expected.contains(&path) {
                    fs::remove_file(path).map_err(|error| error.to_string())?;
                }
            }
        }
        let (_, templates) = self.inputs()?;
        Ok(GeneratedOutput {
            generated_root: root.to_string_lossy().into_owned(),
            templates: templates.into_values().collect(),
            generated: generated.into_keys().collect(),
        })
    }

    /// Generate reachable includes and write a standalone C source to `output`.
    pub fn bundle(&self, entries: &[String], output: &Path) -> Result<BundleOutput> {
        let rendered = self.render(entries)?;
        let (_, templates) = self.inputs()?;
        let bundle = self.source_path(&output.to_string_lossy())?;
        let logical = bundle
            .strip_prefix(&self.root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rendered.graph.includes.contains_key(&logical)
            || rendered.graph.generated.contains(&logical)
        {
            return Err(format!("Bundle output would replace an input: {logical}"));
        }
        let generated_root = self.publish(&rendered.generated)?;
        write_text(&bundle, &rendered.text)?;
        Ok(BundleOutput {
            bundle: bundle.to_string_lossy().into_owned(),
            generated_root: generated_root.to_string_lossy().into_owned(),
            sources: rendered.graph.sources,
            templates: rendered
                .graph
                .generated
                .iter()
                .filter(|name| templates.contains_key(*name))
                .cloned()
                .collect(),
            generated: rendered.graph.generated,
        })
    }
}
