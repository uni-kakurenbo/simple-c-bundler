# c-source-tools

Generate C output functions, compile C assets to assembly, and combine C modules into a standalone C source. Use the CLI for build scripts or the Rust API to configure generation in code.

## Install and try

Build with the repository's Rust 1.99.0 toolchain. Install the CLI from Git:

```sh
cargo install --git https://github.com/uni-kakurenbo/c-source-tools.git --locked
c-source-tools --help
```

To install from a checkout, run `cargo install --path . --locked`. Bundle and compile the included example with a host GCC:

```sh
c-source-tools bundle --root examples/hello --entry src/main.c --entry src/profiles/demo.c --output dist/hello.c
gcc -std=c11 -Wall -Wextra -Wpedantic -Werror examples/hello/dist/hello.c -o examples/hello/dist/hello
./examples/hello/dist/hello
```

The program prints `Hello, world!`. On Windows, GCC adds the `.exe` suffix to the executable.

Pass each entry source with `--entry`. Local includes discover companion `.c` implementations. The bundle contains reachable modules and generated includes and can be compiled as a single C source with a standard compiler.

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

## Rust API

```rust,no_run
use c_source_tools::Build;
use std::path::Path;

let project = Build::new(Path::new("project")).unwrap();
let entries = vec!["src/main.c".into(), "src/composition.c".into()];
let result = project.bundle(&entries, Path::new("dist/program.c")).unwrap();
println!("{}", result.bundle);
```

`Build::new` uses default settings. To load the same JSON configuration as the CLI, deserialize it into `Settings` and pass it to `Build::with_settings`. Use this constructor for assembly recipes or a different project layout.

`bundle` generates reachable assets and writes the standalone source. `resolve` returns the dependency graph, `render` prepares assets and returns the bundle in memory, and `generate` writes all configured generated includes.

Use `Build.environment` for inherited compiler process context, such as session connection variables. These values are excluded from cache keys. Variables that change generated output belong in `Compiler.environment` so changes invalidate the cache.

For individual operations, use these crates:

| Crate | Use it to |
| --- | --- |
| `c-source-bundle` | Discover modules, expand headers, and qualify private C identifiers |
| `c-source-emit` | Convert typed templates or fixed bytes into C output functions |
| `c-source-tools` | Generate assets, invoke the compiler, cache assembly, and write bundles |

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

GCC integration tests compile modular and standalone forms and compare output. Generation tests cover entry points, escaped dependency paths, transitive header changes, cache corruption, unreachable C assets, binary data, and large fixed emitters. Set `CC` to another host GCC-compatible executable for compilation tests.
