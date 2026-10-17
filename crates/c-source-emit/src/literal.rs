//! Encode byte sequences as C string literals and generate fixed-byte output functions.

use crate::Result;
use std::fmt::Write;

/// Encode bytes as a quoted C string literal, including NUL and non-UTF-8 bytes.
pub fn quote(bytes: &[u8]) -> String {
    let mut text = String::from("\"");
    for byte in bytes {
        match byte {
            b'"' => text.push_str("\\\""),
            b'\\' => text.push_str("\\\\"),
            b'?' => text.push_str("\\?"),
            b'\n' => text.push_str("\\n"),
            b'\t' => text.push_str("\\t"),
            b'\r' => text.push_str("\\r"),
            32..=126 => text.push(char::from(*byte)),
            _ => write!(text, "\\{byte:03o}").unwrap(),
        }
    }
    text.push('"');
    text
}

/// Generate a `static inline void name(FILE *output)` function that writes `bytes`.
pub fn function(name: &str, bytes: &[u8]) -> Result<String> {
    let mut chars = name.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!("Invalid C function name: {name}"));
    }

    let mut code = format!("#include <stdio.h>\n\nstatic inline void {name}(FILE *output)\n{{\n");
    if bytes.is_empty() {
        code.push_str("    (void)output;\n");
    }
    // Each decoded literal remains below the minimum C11 limit of 4095 bytes.
    for chunk in bytes.chunks(2048) {
        writeln!(
            code,
            "    fwrite({}, 1, {}, output);",
            quote(chunk),
            chunk.len()
        )
        .unwrap();
    }
    code.push_str("}\n");
    Ok(code)
}
