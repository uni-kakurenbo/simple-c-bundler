//! Compile through a GCC-compatible command; the tool has no execution-environment dependency.

use crate::{Result, read_text, write_text};
use c_source_bundle::modules::Project;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Compiler {
    pub program: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub target: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assembly {
    pub source: String,
    pub include: String,
    pub function: String,
    pub output: String,
    #[serde(default)]
    pub flags: Vec<String>,
    pub compiler: Compiler,
}

#[derive(Deserialize, Serialize)]
struct Cache {
    recipe: String,
    inputs: BTreeMap<String, String>,
    assembly: String,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Compiler {
    fn command(
        &self,
        root: &Path,
        arguments: &[String],
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<Vec<u8>> {
        let result = Command::new(&self.program)
            .current_dir(root)
            .args(&self.arguments)
            .args(arguments)
            .envs(environment)
            .envs(&self.environment)
            .output()
            .map_err(|error| format!("{}: {error}", self.program))?;
        if !result.status.success() {
            return Err(format!(
                "{} {arguments:?}: exit {:?}\n{}",
                self.program,
                result.status.code(),
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        if !result.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&result.stderr));
        }
        Ok(result.stdout)
    }

    fn identity(&self, root: &Path, environment: &BTreeMap<OsString, OsString>) -> Result<Vec<u8>> {
        let target = self.command(root, &["-dumpmachine".into()], environment)?;
        if String::from_utf8_lossy(&target).trim() != self.target {
            return Err(format!(
                "{} must target {}, found {}",
                self.program,
                self.target,
                String::from_utf8_lossy(&target).trim()
            ));
        }
        let mut identity = target;
        identity.extend(self.command(root, &["--version".into()], environment)?);
        Ok(identity)
    }
}

/// GCC makefile dependency escaping, including continued lines and spaces in names.
fn dependencies(text: &str) -> Result<Vec<String>> {
    let text = text.replace("\\\n", "");
    let (_, body) = text
        .split_once(':')
        .ok_or("Invalid compiler dependency file")?;
    let mut words = Vec::new();
    let mut word = String::new();
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => word.push(chars.next().ok_or("Invalid dependency escape")?),
            '$' if chars.peek() == Some(&'$') => {
                chars.next();
                word.push('$');
            }
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.is_empty() {
        return Err("Compiler reported no dependencies".into());
    }
    Ok(words)
}

fn hashes(
    project: &Project,
    names: impl IntoIterator<Item = String>,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for name in names {
        let path = project.source_path(&name)?;
        let canonical = path
            .strip_prefix(&project.root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        result.insert(
            canonical,
            digest(&fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?),
        );
    }
    Ok(result)
}

struct Pending(Vec<PathBuf>);
impl Drop for Pending {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

impl Assembly {
    pub fn generate(
        &self,
        project: &Project,
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<String> {
        project.source_path(&self.source)?;
        let assembly = project.source_path(&self.output)?;
        if !self.output.ends_with(".s") {
            return Err("Assembly output must end in .s".into());
        }
        let derived = |extension: &str| {
            project.source_path(&assembly.with_extension(extension).to_string_lossy())
        };
        let manifest = derived("json")?;
        let lock_path = derived("lock")?;
        let pending_paths = vec![derived("pending.s")?, derived("pending.d")?];
        fs::create_dir_all(assembly.parent().unwrap()).map_err(|error| error.to_string())?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|error| error.to_string())?;
        lock.lock().map_err(|error| error.to_string())?;
        let pending = Pending(pending_paths);

        let mut identity = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        identity.extend(self.compiler.identity(&project.root, environment)?);
        let recipe = digest(&identity);
        if let Ok(cache) = fs::read(&manifest)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<Cache>(&bytes).map_err(|error| error.to_string())
            })
            && cache.recipe == recipe
            && cache.inputs.contains_key(&self.source)
            && hashes(project, cache.inputs.keys().cloned()).ok().as_ref() == Some(&cache.inputs)
            && let Ok(bytes) = fs::read(&assembly)
            && digest(&bytes) == cache.assembly
        {
            return c_source_emit::literal::function(&self.function, &bytes);
        }

        let relative = |path: &Path| {
            path.strip_prefix(&project.root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        };
        let source_before =
            fs::read(project.source_path(&self.source)?).map_err(|error| error.to_string())?;
        let mut arguments = self.flags.clone();
        arguments.extend([
            "-MMD".into(),
            "-MF".into(),
            relative(&pending.0[1]),
            "-S".into(),
            self.source.clone(),
            "-o".into(),
            relative(&pending.0[0]),
        ]);
        self.compiler
            .command(&project.root, &arguments, environment)?;
        let inputs = hashes(project, dependencies(&read_text(&pending.0[1])?)?)?;
        if inputs.get(&self.source) != Some(&digest(&source_before)) {
            return Err("Assembly source changed during compilation; retry generation".into());
        }
        let bytes = fs::read(&pending.0[0]).map_err(|error| error.to_string())?;
        let header = c_source_emit::literal::function(&self.function, &bytes)?;
        fs::rename(&pending.0[0], &assembly).map_err(|error| error.to_string())?;
        let cache = Cache {
            recipe,
            inputs,
            assembly: digest(&bytes),
        };
        write_text(
            &manifest,
            &serde_json::to_string_pretty(&cache).map_err(|error| error.to_string())?,
        )?;
        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependencies_preserve_escaped_spaces_dollars_and_unicode() {
        assert_eq!(
            dependencies("out.s: src/main.c \\\n src/a\\ b.h src/日本語.h src/$$value.h\n")
                .unwrap(),
            ["src/main.c", "src/a b.h", "src/日本語.h", "src/$value.h"]
        );
    }
}
