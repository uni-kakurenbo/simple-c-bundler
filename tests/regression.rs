use simple_c_bundler::bundle::{export, render};
use simple_c_bundler::c_source::{Scanner, directives};
use simple_c_bundler::modules::{Project, module_names};
use simple_c_bundler::{read_text, templates, write_text};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "simple-c-bundler-test-{}-{} 日本語",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self { root };
        fixture.put(
            "src/main.c",
            "#include \"api.h\"\nint main(void) { return answer(); }\n",
        );
        fixture.put(
            "src/api.h",
            "/* before guard */\n#pragma once\nint answer(void);\n",
        );
        fixture.put(
            "src/api.c",
            "#include \"api.h\"\nstatic int value = 42;\nint answer(void) { return value; }\n",
        );
        fixture.put("src/profiles/fixture.c", "int fixture_profile;\n");
        fixture
    }

    fn put(&self, name: &str, text: &str) {
        write_text(&self.root.join(name), text).unwrap();
    }

    fn project(&self) -> Project {
        Project::new(&self.root).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // root is always a dedicated directory immediately below the OS temp dir.
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn declaration_names_and_member_namespace_are_distinct() {
    let text = r#"/* 日本語 value State */
typedef struct State { int value; unsigned flag:1; unsigned :2; } State;
typedef int (*Callback)(void);
enum Kind { BASE = 4, NEXT = (BASE + 1) };
static int value = BASE, second[2] = {1, 2};
#define STEP(x) ((x) + NEXT)
static int get(void) {
    State s = {.value = value, .flag = 1};
    const State *p = &s;
    return STEP(p->value) + second[0];
}
static Callback callback = get;
const char *message = "value State";
_Static_assert(sizeof(int) >= 2, "value");
"#;
    let scanner = Scanner::new(text).unwrap();
    assert_eq!(
        scanner.private_names(),
        [
            "BASE", "Callback", "Kind", "NEXT", "STEP", "State", "callback", "get", "second",
            "value",
        ]
    );
    let rewritten = scanner.rewrite(text, "left__").unwrap();
    assert!(
        rewritten.contains(
            "struct left__State { int value; unsigned flag:1; unsigned :2; } left__State"
        )
    );
    assert!(rewritten.contains(".value = left__value"));
    assert!(rewritten.contains("left__STEP(p->value)"));
    assert!(rewritten.contains("#define left__STEP(x) ((x) + left__NEXT)"));
    assert!(rewritten.contains("/* 日本語 value State */"));
    assert!(rewritten.contains("const char *message = \"value State\";"));
}

#[test]
fn literals_comments_and_multiline_directives_preserve_text() {
    let text = "static int value;\n\
        #define CALL(x) \\\n    ((x) + value)\n\
        const void *a = L\"value\";\n\
        const void *b = u8\"value \\\" quote\";\n\
        char c = '\\''; // value\n";
    let scanner = Scanner::new(text).unwrap();
    let rewritten = scanner.rewrite(text, "module__").unwrap();
    assert!(rewritten.contains("#define module__CALL(x) \\\n    ((x) + module__value)"));
    assert!(rewritten.contains("L\"value\""));
    assert!(rewritten.contains("u8\"value \\\" quote\""));
    assert!(rewritten.contains("char c = '\\''; // value"));
    let directives = directives(
        "/* #include \"false.h\" */\nconst char *s = \"#include\";\n#include <stdio.h>\n",
    )
    .unwrap();
    assert_eq!(directives.len(), 1);
    assert_eq!(directives[0].text, "#include <stdio.h>\n");
}

#[test]
fn pragma_once_is_removed_after_expansion_without_changing_other_text() {
    let fixture = Fixture::new();
    fixture.put(
        "src/main.c",
        "#include \"api.h\"\n#include \"api.h\"\nint main(void) { return answer(); }\n",
    );
    fixture.put(
        "src/api.h",
        "  # pragma /* header */ once // guard\n#include \"detail.h\"\n#pragma pack(push, 1) /* layout\nkept */\nstruct Answer { int value; };\n#pragma pack(pop)\nint answer(void);\n",
    );
    fixture.put(
        "src/detail.h",
        "#pragma \\\n    once /* multiline\n#include \"not-a-header.h\"\n*/\n#include \"api.h\"\n",
    );
    fixture.put(
        "src/api.c",
        "#include \"api.h\"\n/* #pragma once */\nconst char *note = \"#pragma once\";\nint answer(void) { return 42; }\n",
    );

    let text = render(&fixture.project(), "fixture").unwrap().text;
    let remaining: Vec<_> = directives(&text)
        .unwrap()
        .into_iter()
        .filter(|directive| directive.text.contains("pragma"))
        .map(|directive| directive.text)
        .collect();
    assert_eq!(
        remaining,
        [
            "#pragma pack(push, 1) /* layout\nkept */\n",
            "#pragma pack(pop)\n"
        ]
    );
    assert_eq!(text.matches("struct Answer { int value; };").count(), 1);
    assert!(text.contains("/* #pragma once */"));
    assert!(text.contains("const char *note = \"#pragma once\";"));
    assert!(
        read_text(&fixture.root.join("src/api.h"))
            .unwrap()
            .contains("once // guard")
    );
}

#[test]
fn malformed_c_fails_with_a_diagnostic() {
    for (text, expected) in [
        ("/* unfinished", "Unterminated C comment"),
        ("static char *s = \"unfinished", "Unterminated C literal"),
        ("static int broken(", "Unsupported C declaration"),
        (
            "DECLARE_FUNCTION(name, value);",
            "Unsupported C declaration",
        ),
    ] {
        let error = match Scanner::new(text) {
            Ok(_) => panic!("Accepted {text:?}"),
            Err(error) => error,
        };
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn normalized_module_names_are_unique_and_order_independent() {
    let sources: Vec<String> = [
        "src/plugins/io/reader.c",
        "src/ui/input.c",
        "src/left/unit.c",
        "src/right/unit.c",
        "src/a_b.c",
        "src/a/b.c",
        "src/aaa/bb_b/ccc.c",
    ]
    .map(String::from)
    .into();
    let names = module_names(&sources).unwrap();
    assert_eq!(names["src/plugins/io/reader.c"], "plugins__io__reader");
    assert_eq!(names["src/ui/input.c"], "ui__input");
    assert_ne!(names["src/left/unit.c"], names["src/right/unit.c"]);
    assert_eq!(names["src/a_b.c"], "a_b");
    assert_eq!(names["src/a/b.c"], "a__b");
    assert_eq!(names["src/aaa/bb_b/ccc.c"], "aaa__bb_b__ccc");

    let mut reversed = sources;
    reversed.reverse();
    assert_eq!(names, module_names(&reversed).unwrap());
}

#[test]
fn normalized_module_name_collisions_report_both_sources_in_any_order() {
    for (first, second, name) in [
        ("src/a-b.c", "src/a_b.c", "a_b"),
        (
            "src/aaa/bb_b/ccc.c",
            "src/aaa__bb_b__ccc.c",
            "aaa__bb_b__ccc",
        ),
    ] {
        let sources = [first.to_owned(), second.to_owned()];
        let expected =
            format!("Module name collision: {first} and {second} both normalize to {name}");

        assert_eq!(module_names(&sources).unwrap_err(), expected);
        assert_eq!(
            module_names(&[sources[1].clone(), sources[0].clone()]).unwrap_err(),
            expected
        );
    }
}

#[test]
fn template_types_and_repeated_placeholders_keep_argument_order() {
    let code = templates::convert(
        "@show n:int u:uint size:size text:string\n\t%eax \"quote\" path\\file {{text}} {{n}} {{n}} {{u}} {{size}} %s\n"
    ).unwrap();
    assert_eq!(
        code,
        concat!(
            "static void asm_show(FILE *output, int n, unsigned u, size_t size, const char * text)\n",
            "{\n",
            "    fprintf(output,\n",
            "        \"\\t%%eax \\\"quote\\\" path\\\\file %s %d %d %u %zu %%s\\n\", text, n, n, u, size);\n",
            "}\n\n",
        )
    );
    let plain = templates::convert("@plain\n%eax\n").unwrap();
    assert!(plain.contains("fputs(\n        \"%eax\\n\", output)"));
}

#[test]
fn invalid_templates_are_rejected() {
    for (text, expected) in [
        ("", "No templates"),
        ("assembly", "Assembly before"),
        ("@", "Invalid template"),
        ("@bad", "Empty template"),
        ("@bad n:int\n{{missing}}", "Unknown placeholder"),
        ("@bad n:int\n{{n}", "Malformed placeholder"),
        ("@bad\nstray }}", "Malformed placeholder"),
        ("@bad n:int\nunused", "Unused template parameter"),
        ("@bad\n@bad\nbody", "duplicate template"),
        ("@bad x:float\n{{x}}", "Invalid template parameter"),
        ("@bad output:int\n{{output}}", "reserved parameter"),
        ("@bad n:int n:int\n{{n}}", "Duplicate"),
    ] {
        let error = templates::convert(text).unwrap_err();
        assert!(error.contains(expected), "{text:?}: {error}");
    }
}

#[test]
fn discovery_handles_cycles_fallbacks_and_unregistered_modules() {
    let fixture = Fixture::new();
    fixture.put(
        "src/api.h",
        "#ifndef API_H\n#define API_H\n#include \"cycle.h\"\nint answer(void);\n#endif\n",
    );
    fixture.put(
        "src/cycle.h",
        "#ifndef CYCLE_H\n#define CYCLE_H\n#include \"api.h\"\n#endif\n",
    );
    fixture.put(
        "src/profiles/fixture.c",
        "#include \"new/extra.h\"\nint fixture_profile;\n",
    );
    fixture.put("src/new/extra.h", "#include \"api.h\"\nint extra(void);\n");
    fixture.put(
        "src/new/extra.c",
        "#include \"extra.h\"\nint extra(void) { return 1; }\n",
    );
    fixture.put("src/unused.c", "#error Unreachable invalid C\n");
    let graph = fixture.project().resolve("fixture").unwrap();
    assert_eq!(
        graph.sources,
        [
            "src/main.c",
            "src/api.c",
            "src/profiles/fixture.c",
            "src/new/extra.c",
        ]
    );
    assert_eq!(graph.includes["src/new/extra.h"][0].name, "src/api.h");
}

#[test]
fn include_and_profile_errors_are_explicit() {
    let fixture = Fixture::new();
    for (name, text, expected) in [
        (
            "src/main.c",
            "#include \"missing.inc\"\n",
            "Unknown local include",
        ),
        (
            "src/main.c",
            "#if 0\n#include <math.h>\n#endif\n",
            "Conditional include",
        ),
        ("src/main.c", "#include HEADER\n", "Unsupported include"),
        (
            "src/main.c",
            "#include \"../../outside.h\"\n",
            "Source outside src/",
        ),
        ("src/main.c", "#include \"api.h\"\n", ""),
        (
            "src/api.h",
            "#ifndef API_H\n#define API_H\n#else\n#include <math.h>\n#endif\n",
            "header guard alternative",
        ),
    ] {
        fixture.put(name, text);
        if !expected.is_empty() {
            let error = fixture.project().resolve("fixture").unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }
    assert!(
        fixture
            .project()
            .resolve("../escape")
            .unwrap_err()
            .contains("Invalid profile")
    );
    assert!(
        fixture
            .project()
            .resolve("missing")
            .unwrap_err()
            .contains("Unknown profile")
    );
}

#[test]
fn utf8_bom_and_crlf_produce_the_same_bundle() {
    let fixture = Fixture::new();
    fixture.put(
        "src/api.c",
        "/* 日本語 */\n#include \"api.h\"\nint answer(void) { return 42; }\n",
    );
    let first = render(&fixture.project(), "fixture").unwrap().text;
    for source in [
        "src/main.c",
        "src/api.h",
        "src/api.c",
        "src/profiles/fixture.c",
    ] {
        let path = fixture.root.join(source);
        let text = read_text(&path).unwrap();
        fs::write(path, format!("\u{feff}{}", text.replace('\n', "\r\n"))).unwrap();
    }

    let second = render(&fixture.project(), "fixture").unwrap().text;
    assert_eq!(first, second);
    assert!(second.contains("/* 日本語 */"));
    assert!(!second.contains('\r'));
}

#[test]
fn failed_validation_preserves_previous_outputs() {
    let fixture = Fixture::new();
    let output = export(&fixture.root, "fixture").unwrap();
    let original = read_text(Path::new(&output.bundle)).unwrap();
    fixture.put(
        "src/api.c",
        "#include \"api.h\"\n#include \"bad.inc\"\nint answer(void) { return 42; }\n",
    );
    fixture.put("src/bad.asm", "@bad n:int\n{{missing}}\n");
    assert!(
        export(&fixture.root, "fixture")
            .unwrap_err()
            .contains("Unknown placeholder")
    );
    assert_eq!(original, read_text(Path::new(&output.bundle)).unwrap());
    assert!(
        !fixture
            .root
            .join("build/generated/fixture/bad.inc")
            .exists()
    );
}

#[test]
fn cli_rejects_name_collisions_without_overwriting_outputs() {
    let fixture = Fixture::new();
    fixture.put(
        "src/api.c",
        "#include \"api.h\"\n#include \"message.inc\"\nint answer(void) { return 42; }\n",
    );
    fixture.put("src/message.asm", "@show\noriginal\n");

    let output = export(&fixture.root, "fixture").unwrap();
    let bundle = Path::new(&output.bundle);
    let generated = Path::new(&output.generated_root).join("message.inc");
    let original_bundle = read_text(bundle).unwrap();
    let original_generated = read_text(&generated).unwrap();

    fixture.put("src/a-b.h", "int left(void);\n");
    fixture.put("src/a-b.c", "int left(void) { return 1; }\n");
    fixture.put("src/a_b.h", "int right(void);\n");
    fixture.put("src/a_b.c", "int right(void) { return 2; }\n");
    fixture.put(
        "src/profiles/fixture.c",
        "#include \"a-b.h\"\n#include \"a_b.h\"\nint fixture_profile;\n",
    );
    fixture.put("src/message.asm", "@show\nupdated\n");

    let failed = Command::new(env!("CARGO_BIN_EXE_simple-c-bundler"))
        .current_dir(std::env::temp_dir())
        .arg("bundle")
        .arg("--root")
        .arg(&fixture.root)
        .args(["--profile", "fixture"])
        .output()
        .unwrap();

    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert_eq!(
        String::from_utf8(failed.stderr).unwrap().trim(),
        "Module name collision: src/a-b.c and src/a_b.c both normalize to a_b"
    );
    assert_eq!(read_text(bundle).unwrap(), original_bundle);
    assert_eq!(read_text(&generated).unwrap(), original_generated);
}

#[test]
fn cli_manifest_paths_and_diagnostics_work_from_another_directory() {
    let fixture = Fixture::new();
    let command = |action: &str, profile: &str| {
        Command::new(env!("CARGO_BIN_EXE_simple-c-bundler"))
            .current_dir(std::env::temp_dir())
            .arg(action)
            .arg("--root")
            .arg(&fixture.root)
            .arg("--profile")
            .arg(profile)
            .output()
            .unwrap()
    };

    let resolved = command("resolve", "fixture");
    assert!(resolved.status.success());
    assert!(resolved.stderr.is_empty());
    assert!(!fixture.root.join("dist").exists());
    let bundled = command("bundle", "fixture");
    assert!(bundled.status.success());
    assert!(bundled.stderr.is_empty());
    let manifest: serde_json::Value = serde_json::from_slice(&bundled.stdout).unwrap();
    assert!(Path::new(manifest["bundle"].as_str().unwrap()).is_file());
    assert!(
        manifest["generatedRoot"]
            .as_str()
            .unwrap()
            .ends_with("fixture")
    );
    let bad = command("bundle", "../escape");
    assert_eq!(bad.status.code(), Some(1));
    assert!(bad.stdout.is_empty());
    assert!(
        String::from_utf8(bad.stderr)
            .unwrap()
            .contains("Invalid profile")
    );
}
