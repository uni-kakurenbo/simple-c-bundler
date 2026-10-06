use simple_c_bundler::bundle;
use simple_c_bundler::{read_text, write_text};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "simple-c-bundler-gcc-{}-{timestamp} 日本語",
            std::process::id()
        ));

        Self(root)
    }

    fn put(&self, path: &str, text: &str) {
        write_text(&self.0.join(path), text).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // This guard owns a unique directory immediately inside the OS temp directory.
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn checked(command: &mut Command) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("Could not execute {command:?}: {error}"));
    assert!(
        output.status.success(),
        "{command:?}: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    output
}

#[test]
fn modular_and_bundled_sources_compile_and_preserve_template_output() {
    let fixture = Fixture::new();
    macro_rules! put {
        ($path:literal) => {
            fixture.put($path, include_str!(concat!("fixtures/modules/", $path)))
        };
    }
    put!("src/api.h");
    put!("src/left/message.asm");
    put!("src/left/unit.h");
    put!("src/left/unit.c");
    put!("src/right/message.asm");
    put!("src/right/unit.h");
    put!("src/right/unit.c");
    put!("src/main.c");
    fixture.put("src/profiles/fixture.c", "int fixture_profile;\n");

    let root = &fixture.0;
    let output = bundle::export(root, "fixture").unwrap();
    assert!(
        !read_text(Path::new(&output.bundle))
            .unwrap()
            .contains("#pragma once")
    );
    let generated = Path::new(&output.generated_root);
    let compiler = std::env::var_os("CC")
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "gcc".into());
    let flags = ["-std=c11", "-Wall", "-Wextra", "-Wpedantic", "-Werror"];
    let mut objects = Vec::new();

    for source in &output.sources {
        let object = root.join(format!("objects/{source}.o"));
        fs::create_dir_all(object.parent().unwrap()).unwrap();
        let local_generated = generated.join(Path::new(&source[4..]).parent().unwrap());

        checked(
            Command::new(&compiler)
                .current_dir(root)
                .args(flags)
                .arg("-Isrc")
                .arg(format!("-I{}", generated.display()))
                .arg(format!("-I{}", local_generated.display()))
                .arg("-c")
                .arg(source)
                .arg("-o")
                .arg(&object),
        );
        objects.push(object);
    }

    let extension = if cfg!(windows) { "exe" } else { "bin" };
    let modular = root.join(format!("modular.{extension}"));
    checked(
        Command::new(&compiler)
            .current_dir(root)
            .args(&objects)
            .arg("-o")
            .arg(&modular),
    );

    let standalone = root.join("isolated/program.c");
    write_text(&standalone, &read_text(Path::new(&output.bundle)).unwrap()).unwrap();
    let bundled = standalone.with_extension(extension);
    checked(
        Command::new(&compiler)
            .current_dir(root)
            .args(flags)
            .arg(&standalone)
            .arg("-o")
            .arg(&bundled),
    );

    for binary in [modular, bundled] {
        let result = checked(Command::new(&binary).current_dir(root));
        assert!(result.stderr.is_empty());
        assert_eq!(
            String::from_utf8(result.stdout)
                .unwrap()
                .replace("\r\n", "\n"),
            "value State 7 9\n%eax \"quoted\" path\\file 42 42 %s\nother 11\n"
        );
    }
}
