# c-source-tools

Generate C output functions, compile C assets to assembly, and combine ordinary C modules into a standalone C source. The CLI and Rust API use the same implementation. No WSLC, container, or execution-platform library is required.

## Install and try

The repository pins Rust 1.99.0. Install the CLI from this checkout:

```sh
cargo install --path . --locked
c-source-tools --help
c-source-tools bundle --root examples/hello --entry src/main.c --entry src/profiles/demo.c --output dist/hello.c
gcc -std=c11 -Wall -Wextra -Wpedantic -Werror examples/hello/dist/hello.c -o examples/hello/build/hello
```

Create the executable output directory before invoking GCC, and use an `.exe` extension on Windows. The program prints `Hello, world!`.

Entry points are explicit: neither `main.c` nor a profile directory is implicit. Local includes discover companion `.c` implementations. Only reachable modules and generated includes enter the bundle. The resulting C source needs neither this tool nor the original project to compile or run.

## Commands

```text
c-source-tools bundle --root DIR --entry FILE [--entry FILE...] --output FILE [--config FILE] [--compiler PROGRAM]
c-source-tools resolve --root DIR --entry FILE [--entry FILE...] [--config FILE]
c-source-tools generate --root DIR [--config FILE] [--compiler PROGRAM]
c-source-tools template FILE
```

`bundle` prepares reachable generated includes and writes the standalone C file. `generate` prepares all declared assets and templates without parsing the C composition roots. `resolve` prints the graph without compiling or writing files. `template` converts a single typed template on stdout. Other successful commands return JSON manifests. Diagnostics go to stderr; failures return exit code 1.

`--root` defaults to the working directory. Configuration paths are relative to the root. An explicit missing configuration is an error. Otherwise `c-source-tools.json` is optional. `--compiler` overrides the program of all assembly recipes; compiler arguments remain ordinary GCC arguments.

## Configuration

Without configuration, includes and templates are searched under `src/`, and generated headers go under `build/generated/include/`. Supply other include and template directories for a different project layout.

```json
{
  "includeDirs": ["src", "include"],
  "templateDirs": ["src"],
  "generatedRoot": "build/generated/include",
  "assemblies": [
    {
      "source": "src/runtime/image.c",
      "include": "src/runtime/embedded.h",
      "function": "EmitRuntime",
      "output": "build/generated/runtime/runtime.s",
      "flags": ["-std=c11", "-O2", "-Isrc"],
      "compiler": {
        "program": "gcc",
        "arguments": [],
        "environment": {},
        "target": "x86_64-linux-gnu"
      }
    }
  ]
}
```

`include` is the logical project-relative header name. It may be registered before the file exists; it is never written into the source directory. The example writes `build/generated/include/src/runtime/embedded.h`. For a modular build, add generated include roots such as `-Ibuild/generated/include/src` alongside `-Isrc`. A source using a local include may also need its corresponding generated source directory on the include path.

`output` names the assembly file. Adjacent `.json` metadata records the recipe, compiler identity, input hashes, and assembly hash; `.lock` serializes concurrent generation of the same artifact. Both assembly and generated include output belong in dedicated build directories. `generate` removes obsolete `.h` and `.inc` files only inside its generated include directory, after preparing all inputs successfully. Keep handwritten files outside that directory.

A recipe invokes a configurable GCC-compatible executable with `-S`; it does not run target code. Its `-dumpmachine` result must match `target`. Use a native GCC when it targets the desired environment, a cross GCC otherwise, or an external wrapper such as `wslc-gcc`. All compiler source and output arguments are project-relative, with forward slashes.

GCC `-MMD` dependencies track transitive project headers, including C files included into a translation unit. Changes to those inputs, recipe settings, configured compiler environment, target/version identity, or the cached assembly invalidate the cache. Compiler identity is queried even on a cache hit. Pin the toolchain environment or clear cached assembly metadata after changing its system headers independently. Artifact-affecting environment variables belong in `compiler.environment`.

## Rust API and crate boundaries

```rust,no_run
use c_source_tools::Build;
use std::path::Path;

let project = Build::new(Path::new("project")).unwrap();
let entries = vec!["src/main.c".into(), "src/composition.c".into()];
let result = project.bundle(&entries, Path::new("dist/program.c")).unwrap();
println!("{}", result.bundle);
```

Use `Build::with_settings` for declared C assets or a different layout. `bundle` includes generation; no separate planning or compiler trait implementation is required. `Build.environment` can supply inherited process context, such as a session connection, without changing global environment variables. Semantic compiler configuration belongs in the recipe.

| Crate | Responsibility | Dependencies on execution environments |
| --- | --- | --- |
| `c-source-bundle` | C scanner, module graph, identifier qualification, header expansion | None |
| `c-source-emit` | Typed templates and fixed-byte C output functions | None |
| `c-source-tools` | Compiler invocation, dependency cache, generated includes, publication, CLI | External compiler command only |

The bundle crate accepts generic registered generated includes. It does not interpret `.asm` or compiler recipes. The emit crate transforms supplied data without starting processes or choosing output directories. The facade discovers templates and connects these pieces.

## Names and supported C

Private functions, variables, types, tags, enum constants, and macros receive stable source-derived prefixes. Structure members, public names, strings, and comments keep their meaning. Paths that normalize to the same module prefix are an error.

This is not a general C preprocessor. Local includes must be unconditional except for ordinary header guards. Conditional includes, macro-generated declarations, and unsupported compiler-specific declarations produce diagnostics. Source paths and symlinks must stay within the project root. Generated includes are self-contained leaves and may include standard headers.

## Typed templates and fixed text

```asm
@load value:int
    movl ${{value}}, %eax
```

This declares `static void asm_load(FILE *output, int value)`. Parameter types are `int`, `uint`, `size`, and `string`. Repeated placeholders retain their order; unused parameters, duplicate names, and unknown placeholders are errors. File-level blank lines and comments beginning with `#` before the first declaration are omitted. Templates include their own `<stdio.h>` declarations.

`c_source_emit::literal::function(name, bytes)` creates a C function that writes fixed bytes with `fwrite`. It handles NUL and non-UTF-8 bytes and splits long data below C11's minimum literal-size limit. Templates and fixed text share C escaping. The writer does not hide FILE errors; callers can check `ferror` according to their own output policy.

## Verification

```sh
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Tests use synthetic modules and templates. GCC integration tests compile modular and standalone forms and compare output. Generation tests cover arbitrary entry points, escaped dependency paths, transitive header changes, cache corruption, unreachable C assets, binary data, and large fixed emitters. Set `CC` to another host GCC-compatible executable for compilation tests. No consuming project or WSLC installation is required by these tests.

## Install from Git

```sh
cargo install --git https://github.com/uni-kakurenbo/c-source-tools --locked
```
