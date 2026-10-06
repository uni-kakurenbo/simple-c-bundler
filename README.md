# simple-c-bundler

A Rust CLI that combines ordinary C modules into one standalone C file. It discovers modules through local includes, qualifies private identifiers by their source paths, and converts typed `.asm` templates into C output functions.

## Build and install

The repository pins the official Rust 1.99.0 toolchain. Cargo installs and runs the CLI directly.

```sh
cargo build --locked
cargo install --path . --locked
cargo install --git https://github.com/uni-kakurenbo/simple-c-bundler --locked
```

The installed `simple-c-bundler` executable works from any directory; `--root` selects the input project. The tool does not require another repository, a project configuration file, a shell script, or a runtime service.

GCC is used only by the compilation integration test and by the example commands below. Set `CC` to another C compiler executable if needed.

## Try the example

```sh
cargo run --locked -- bundle --root examples/hello --profile demo
gcc -std=c11 -Wall -Wextra -Wpedantic -Werror examples/hello/dist/demo.c -o examples/hello/build/hello
./examples/hello/build/hello
```

The executable prints `Hello, world!`. On Windows, use an output path ending in `.exe` when compiling the example.

## Commands

```text
simple-c-bundler bundle   --root DIRECTORY --profile NAME
simple-c-bundler resolve  --root DIRECTORY --profile NAME
simple-c-bundler template FILE
```

`bundle` writes `dist/NAME.c` and generated template headers under `build/generated/NAME/`. It returns a JSON manifest containing the output paths and selected source and template files. Input validation finishes before outputs are written.

`resolve` prints the dependency graph as JSON without writing files. `template` converts one template to C and writes it to standard output. Diagnostics go to standard error, and errors return exit code 1.

## Project layout

Each input project has a `src/` directory and one or more composition roots:

```text
src/
  main.c
  library.h
  library.c
  message.asm
  profiles/
    demo.c
```

The entry points are `src/main.c` and `src/profiles/NAME.c`. A profile name starts with a lowercase ASCII letter and contains only lowercase ASCII letters, digits, or underscores. Local includes are resolved relative to the including file and then relative to `src/`.

When a selected header has a `.c` file beside it, that implementation is included automatically. Headers without a corresponding implementation provide only declarations. An include ending in `.inc` is generated from the `.asm` file with the same path and stem. Only reachable modules and templates are included; build registration and a project configuration file are unnecessary.

Headers can use `#pragma once` for modular builds. Bundling expands each reachable header once and removes its `#pragma once` directive, including an indented or continued form. Other pragmas and occurrences inside comments or string literals retain their text. Input files are unchanged.

## Private names

Private functions, variables, typedef names, tags, enum constants, and macros receive a source-specific prefix. Directory separators become `__`, the `.c` extension and leading `src/` are removed, and the original identifier is appended with `__`.

For example, `value` in `src/aaa/bb_b/ccc.c` becomes `aaa__bb_b__ccc__value`. Underscores already present in file and directory names are preserved. Other characters that are not ASCII letters or digits become underscores.

If different source paths normalize to the same module name, bundling fails and reports both paths and the conflicting name. No numeric suffix is added. Public identifiers, structure members, string literals, and comments retain their text. Each module's generated comment records the private identifier mapping.

## Typed templates

```asm
@load immediate:int
    movl ${{immediate}}, %eax
```

This generates `static void asm_load(FILE *output, int immediate)`. Template parameters use `int`, `uint`, `size`, or `string`, and `{{name}}` supplies the corresponding C argument. Repeated placeholders preserve argument order. Percent signs, quotes, backslashes, tabs, and line endings are escaped for C output. Template names, parameters, and placeholders are validated; duplicate names, unknown placeholders, and unused parameters are errors.

Templates are converted ahead of time. The bundled C file has no runtime dependency on the Rust tool, its input headers, or template files.

## Supported C

The scanner handles ordinary C declarations and distinguishes structure members from other identifiers. It preserves untouched text using UTF-8 byte offsets. Input may have a UTF-8 BOM or CRLF line endings; output is BOM-free UTF-8 with LF line endings.

This tool does not run a general C preprocessor. Local includes must be unconditional except for ordinary header guards. Conditional includes, macro-generated declarations, and unsupported compiler-specific declaration syntax produce diagnostics. Source paths and symlinks must remain inside the input project's `src/` directory.

## Tests

```sh
cargo test --locked
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests use synthetic C modules and templates. The GCC integration test compiles both the modular and bundled forms, then compares their output. It covers duplicate private names in separate modules, generated headers with identical basenames, function pointers, member names, and template escaping.
