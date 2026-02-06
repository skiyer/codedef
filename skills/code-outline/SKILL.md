---
name: code-outline
description: Extract code definitions and generate file outlines from source files using tree-sitter. Use for code files to retrieve complete definitions or view structure; use `read` for non-code files.
---

# Code Outline

Code Outline extracts code definitions from source files using tree-sitter. It supports two main operations: finding the definition at a specific line, and listing all definitions in a file.

## Setup

```bash
cargo install --git https://github.com/skiyer/code-outline.git
```

## Find Definition at Line

Find the innermost enclosing definition for a given line number.

```bash
code-outline at <FILE_PATH> <LINE_NUMBER> [OPTIONS]
```

**Options:**
- `-l, --lang <LANG>` - Programming language (auto-detected if not specified)
- `--show-type` - Show the type of definition found

**Examples:**
```bash
# Find function containing line 42 (auto-detect language)
code-outline at src/main.c 42

# Specify language explicitly
code-outline at src/main.c 42 --lang c

# Show definition type
code-outline at src/main.c 42 --show-type
```

**Output:**
```
# function_definition starting at line 22
22. int add(int a, int b) {
23.     return a + b;
24. }
```

## List Definitions (All)

List all definitions in a file with line numbers and signatures.

```bash
code-outline all <FILE_PATH> [OPTIONS]
```

**Options:**
- `-l, --lang <LANG>` - Programming language (auto-detected if not specified)

**Examples:**
```bash
# List all definitions
code-outline all src/main.c
```

**Output:**
```
 3: [macro  ] #define MAX_SIZE 100
 4: [macro  ] #define SQUARE(x) ((x) * (x))
 6: [struct ] struct Point
11: [typedef] typedef struct { ... } Rectangle
16: [enum   ] enum Color
22: [fn     ] int add(int a, int b)
```

**Definition Type Tags:**
- `fn` - Function definition
- `struct` - Struct specifier
- `union` - Union specifier
- `enum` - Enum specifier
- `typedef` - Type definition
- `macro` - Preprocessor macro (#define)

## Supported Languages

| Language | Extensions | Definition Types |
|----------|------------|------------------|
| C | `.c`, `.h` | function, struct, union, enum, typedef, macro |

## When to Use

### Use code-outline for code files
- **Source code files** (e.g., `.c`, `.h`, `.py`, `.js`, `.rs`, `.go`, `.java`): Use `code-outline at` to retrieve complete function/type implementations, or `code-outline all` to view the file structure.
- **Why:** Code Outline uses tree-sitter parsing to accurately identify definition boundaries and extract the complete implementation. This avoids issues where the `read` tool might truncate content due to line limits (2000 lines/50KB), cutting off the middle of a function.

### Use `read` for non-code files
- **Configuration files** (e.g., `.json`, `.yaml`, `.toml`, `.xml`): Use `read`
- **Documentation** (e.g., `.md`, `.txt`, `.rst`): Use `read`
- **Data files** (e.g., `.csv`, log files): Use `read`
- **Why:** Code Outline is designed for parsing code syntax trees. It does not provide meaningful output for non-code content.

### Fallback
If `code-outline` fails on a code file (e.g., unsupported language, parsing error), fall back to using the `read` tool.

---

## Tips

### Getting the Big Picture

Before diving into specific functions, get an overview of the file structure:

```bash
code-outline all src/main.c
```

This shows all definitions with line numbers, helping you identify where to look next.

### Finding the Right Definition

When you know a line number but don't know which function it belongs to:

```bash
code-outline at src/main.c 150
```

This returns the complete function/type that contains line 150.

### Dealing with Large Files

For files that exceed `read` tool limits, use `code-outline at` to extract specific definitions without loading the entire file:

```bash
# Find what contains the line you're interested in
code-outline at large_file.c 2500

# This returns just that function, not the whole file
```

### Language Detection

Code Outline auto-detects language from file extension. Force a specific language if needed:

```bash
code-outline at source_file 42 --lang c
code-outline all header_file --lang c
```

### Quick Reference

| Task | Command |
|------|---------|
| Find what contains line 42 | `code-outline at file.c 42` |
| List all definitions | `code-outline all file.c` |
| Show with type info | `code-outline at file.c 42 --show-type` |
| Force language | `code-outline all file.c --lang c` |

### Notes

- Line numbers are 1-based (first line is line 1)
- Language is auto-detected from file extension when not specified
- For typedefs with struct/union/enum bodies, the outline shows `{ ... }` placeholder
