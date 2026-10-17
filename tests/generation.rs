use c_source_tools::{
    Build, Settings,
    compiler::{Assembly, Compiler},
    write_text,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "c-source-generation-{}-{stamp} 日本語",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn put(&self, path: &str, text: &str) {
        write_text(&self.0.join(path), text).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn c_assets_use_arbitrary_entries_track_headers_and_compile_without_source_artifacts() {
    let fixture = Fixture::new();
    fixture.put(
        "code/runtime.c",
        "#include \"value space.h\"\nint answer(void) { return VALUE; }\n",
    );
    fixture.put("code/value space.h", "#define VALUE 37\n");
    fixture.put("app/program.c", "#include \"generated/runtime.h\"\nint main(void) { FILE *f = fopen(\"actual.s\", \"wb\"); if (!f) return 1; Emit(f); return fclose(f) != 0; }\n");
    fixture.put("app/plain.c", "int main(void) { return 0; }\n");
    let compiler = std::env::var("CC").unwrap_or_else(|_| "gcc".into());
    let target = Command::new(&compiler)
        .arg("-dumpmachine")
        .output()
        .unwrap();
    assert!(target.status.success());
    let recipe = Assembly {
        source: "code/runtime.c".into(),
        include: "app/generated/runtime.h".into(),
        function: "Emit".into(),
        output: "build/runtime.s".into(),
        flags: vec!["-std=c11".into(), "-Icode".into()],
        compiler: Compiler {
            program: compiler.clone(),
            arguments: Vec::new(),
            environment: BTreeMap::new(),
            target: String::from_utf8(target.stdout).unwrap().trim().into(),
        },
    };
    let mut settings = Settings {
        include_dirs: vec!["app".into(), "code".into()],
        template_dirs: Vec::new(),
        assemblies: vec![recipe],
        ..Settings::default()
    };
    let build = Build::with_settings(&fixture.0, settings.clone()).unwrap();
    let entries = vec!["app/program.c".into()];
    let output = build.bundle(&entries, Path::new("dist/program.c")).unwrap();
    assert!(!fixture.0.join("app/generated/runtime.h").exists());
    assert!(
        Path::new(&output.generated_root)
            .join("app/generated/runtime.h")
            .is_file()
    );
    let original = fs::read(fixture.0.join("build/runtime.s")).unwrap();
    let timestamp = fs::metadata(fixture.0.join("build/runtime.s"))
        .unwrap()
        .modified()
        .unwrap();
    build.bundle(&entries, Path::new("dist/program.c")).unwrap();
    assert_eq!(
        timestamp,
        fs::metadata(fixture.0.join("build/runtime.s"))
            .unwrap()
            .modified()
            .unwrap()
    );

    let extension = if cfg!(windows) { "exe" } else { "bin" };
    for (name, source) in [("modular", "app/program.c"), ("bundled", "dist/program.c")] {
        let binary = fixture.0.join(format!("{name}.{extension}"));
        let compiled = Command::new(&compiler)
            .current_dir(&fixture.0)
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Wpedantic",
                "-Werror",
                "-Ibuild/generated/include/app",
            ])
            .arg(source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        assert!(
            Command::new(binary)
                .current_dir(&fixture.0)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(original, fs::read(fixture.0.join("actual.s")).unwrap());
    }

    fixture.put("code/value space.h", "#define VALUE 42\n");
    build.bundle(&entries, Path::new("dist/program.c")).unwrap();
    let updated = fs::read(fixture.0.join("build/runtime.s")).unwrap();
    assert_ne!(original, updated);
    fs::write(
        fixture.0.join("build/runtime.s"),
        "corrupted cached assembly",
    )
    .unwrap();
    build.bundle(&entries, Path::new("dist/program.c")).unwrap();
    assert_eq!(
        updated,
        fs::read(fixture.0.join("build/runtime.s")).unwrap()
    );

    // Unreachable C assets do not require an installed target compiler.
    settings.assemblies[0].compiler.program = "missing-compiler".into();
    let plain = Build::with_settings(&fixture.0, settings).unwrap();
    plain
        .bundle(&["app/plain.c".into()], Path::new("dist/plain.c"))
        .unwrap();
}

#[test]
fn fixed_emitters_handle_binary_data_and_more_than_one_c_literal() {
    let fixture = Fixture::new();
    let mut bytes = vec![0, 255, b'"', b'\\', b'\n'];
    bytes.extend(vec![b'x'; 12_000]);
    fixture.put(
        "emitter.h",
        &c_source_tools::literal::function("Emit", &bytes).unwrap(),
    );
    fixture.put("main.c", "#include \"emitter.h\"\nint main(void) { FILE *f = fopen(\"actual\", \"wb\"); if (!f) return 1; Emit(f); return fclose(f) != 0; }\n");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| "gcc".into());
    let binary = fixture
        .0
        .join(if cfg!(windows) { "main.exe" } else { "main" });
    let result = Command::new(compiler)
        .current_dir(&fixture.0)
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Wpedantic",
            "-Werror",
            "main.c",
            "-o",
        ])
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        Command::new(binary)
            .current_dir(&fixture.0)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(fixture.0.join("actual")).unwrap(), bytes);
}
