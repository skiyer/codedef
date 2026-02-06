# code-outline

Extract code definitions (function, struct, class, enum, etc.) from source files using tree-sitter.

## Supported Languages

| Language | Extensions | Definition Types |
|----------|------------|------------------|
| C | `.c`, `.h` | function, struct, union, enum, typedef, macro |

More languages coming soon: C++, Rust, Go, Python, JavaScript/TypeScript, Java...

## Features

- Parse source code using tree-sitter for accurate AST-based extraction
- Find the innermost enclosing definition for a given line number
- List all definitions in a file (outline view with line numbers)
- Auto-detect language from file extension
- Single static binary with no runtime dependencies

## Installation

### From Source

```bash
cargo install --path .
```

### Build Release Binary

```bash
cargo build --release
# Binary at: target/release/code-outline
```

## Usage

code-outline 提供两个子命令：`at` 和 `all`。

### At - 查找指定行的定义

```bash
code-outline at <FILE_PATH> <LINE_NUMBER> [OPTIONS]

Arguments:
  <FILE_PATH>    Path to the source file
  <LINE_NUMBER>  Line number (1-based) to find the enclosing definition for

Options:
  -l, --lang <LANG>  Programming language [possible values: c]
      --show-type    Show the type of definition found
  -h, --help         Print help
  -V, --version      Print version
```

#### Examples

```bash
# Find the function containing line 42 (auto-detect language)
code-outline at src/main.c 42

# Explicitly specify language
code-outline at src/main.c 42 --lang c

# Show definition type
code-outline at src/main.c 42 --show-type
```

### All - 列出文件所有定义

```bash
code-outline all <FILE_PATH> [OPTIONS]

Arguments:
  <FILE_PATH>    Path to the source file

Options:
  -l, --lang <LANG>  Programming language [possible values: c]
  -h, --help         Print help
```

#### Examples

```bash
# List all definitions in a file
code-outline all src/main.c

# Output format:
#  3: [macro  ] #define MAX_SIZE 100
#  6: [struct ] struct Point
# 11: [typedef] typedef struct { ... } Rectangle
# 22: [fn     ] int add(int a, int b)
```

## Docker

Build a minimal Docker image:

```bash
docker build -t code-outline .
docker run --rm -v $(pwd):/src code-outline at /src/test.c 10
docker run --rm -v $(pwd):/src code-outline all /src/test.c
```

## Adding New Languages

To add support for a new language:

1. Add the tree-sitter grammar dependency to `Cargo.toml`
2. Add a new variant to the `Lang` enum
3. Implement `tree_sitter_language()`, `definition_types()`, `compound_types()`, and `body_types()` for the new language
4. Update `from_extension()` to recognize the file extensions

## License

MIT
