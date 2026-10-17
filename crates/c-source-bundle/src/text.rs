use std::fs;
use std::path::Path;

use crate::Result;

pub fn read_text(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    Ok(text.trim_start_matches('\u{feff}').replace("\r\n", "\n"))
}

pub fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(path, text.replace("\r\n", "\n")).map_err(|e| format!("{}: {e}", path.display()))
}
