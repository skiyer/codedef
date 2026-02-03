//! Extract code definitions from source files using tree-sitter.
//!
//! This tool parses source code and finds the innermost enclosing
//! definition (function, struct, class, enum, etc.) for a given line number.
//!
//! Currently supported languages:
//! - C

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use tree_sitter::{Language, Node, Parser as TsParser};

/// Maximum depth for definition search to prevent stack overflow
const MAX_DEFINITION_SEARCH_DEPTH: usize = 128;

/// Supported programming languages
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum Lang {
    /// C language
    #[default]
    C,
}

impl Lang {
    /// Get tree-sitter language
    fn tree_sitter_language(self) -> Language {
        match self {
            Self::C => Language::new(tree_sitter_c::LANGUAGE),
        }
    }

    /// Get definition types for this language
    fn definition_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &[
                "function_definition",
                "type_definition",       // typedef
                "preproc_def",           // #define
                "preproc_function_def",  // #define with parameters
            ],
        }
    }

    /// Get compound types that need body check
    fn compound_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &[
                "struct_specifier",
                "union_specifier",
                "enum_specifier",
            ],
        }
    }

    /// Get body node types for compound types
    fn body_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &[
                "field_declaration_list",
                "enumerator_list",
            ],
        }
    }

    /// Detect language from file extension
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "c" | "h" => Some(Self::C),
            _ => None,
        }
    }
}

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "codedef")]
#[command(author, version, about = "Extract code definitions from source files using tree-sitter", long_about = None)]
struct Args {
    /// Path to the source file
    file_path: PathBuf,

    /// Line number (1-based) to find the enclosing definition for
    line_number: usize,

    /// Programming language (auto-detected from extension if not specified)
    #[arg(short, long, value_enum)]
    lang: Option<Lang>,

    /// Show the type of definition found
    #[arg(long)]
    show_type: bool,
}

/// Represents a found definition
#[derive(Debug)]
struct Definition {
    code: String,
    start_line: usize,
    def_type: String,
    size: usize,
    is_typedef_child: bool,
}

/// Check if a node contains the target row
fn contains_row(node: &Node, target_row: usize) -> bool {
    let start_row = node.start_position().row;
    let end_row = node.end_position().row;
    let end_col = node.end_position().column;

    // Quick rejections
    if target_row < start_row || target_row > end_row {
        return false;
    }

    // If the end lands at column 0, the row itself is excluded
    if target_row == end_row && end_col == 0 {
        return false;
    }

    true
}

/// Check if a node is a definition type
fn is_definition_type(node_type: &str, lang: Lang) -> bool {
    lang.definition_types().contains(&node_type)
}

/// Check if a node is a compound type
fn is_compound_type(node_type: &str, lang: Lang) -> bool {
    lang.compound_types().contains(&node_type)
}

/// Check if a compound type has a body
fn has_body(node: &Node, lang: Lang) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if lang.body_types().contains(&child.kind()) {
            return true;
        }
    }
    false
}

/// Traverse the AST and collect matching definitions
fn traverse(
    node: Node<'_>,
    source_code: &str,
    target_row: usize,
    depth: usize,
    definitions: &mut Vec<Definition>,
    lang: Lang,
    is_parent_typedef: bool,
) {
    if depth >= MAX_DEFINITION_SEARCH_DEPTH {
        return;
    }

    if !contains_row(&node, target_row) {
        return;
    }

    let node_type = node.kind();
    let mut is_definition = false;
    let mut mark_compound_child = false;

    if is_definition_type(node_type, lang) {
        is_definition = true;
        if node_type == "type_definition" {
            mark_compound_child = true;
        }
    } else if is_compound_type(node_type, lang) && has_body(&node, lang) {
        is_definition = true;
    }

    if is_definition {
        let code = source_code
            .get(node.start_byte()..node.end_byte())
            .unwrap_or("")
            .to_string();
        let start_line = node.start_position().row + 1;
        let size = node.end_byte() - node.start_byte();

        definitions.push(Definition {
            code,
            start_line,
            def_type: node_type.to_string(),
            size,
            is_typedef_child: is_parent_typedef && is_compound_type(node_type, lang),
        });
    }

    // Continue searching children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse(
            child,
            source_code,
            target_row,
            depth + 1,
            definitions,
            lang,
            mark_compound_child || is_parent_typedef,
        );
    }
}

/// Find the innermost definition for a given line number
fn find_innermost_definition(
    file_path: &PathBuf,
    line_number: usize,
    lang: Lang,
) -> Result<Option<(String, usize, String)>> {
    // Read source file
    let source_code = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    // Initialize tree-sitter parser
    let mut parser = TsParser::new();
    let language = lang.tree_sitter_language();
    parser
        .set_language(&language)
        .context("Failed to set language for parser")?;

    // Parse the source code
    let tree = parser
        .parse(&source_code, None)
        .context("Failed to parse source code")?;

    let target_row = line_number - 1; // Convert to 0-indexed

    let mut definitions = Vec::new();

    // Traverse the AST
    traverse(
        tree.root_node(),
        &source_code,
        target_row,
        0,
        &mut definitions,
        lang,
        false,
    );

    if definitions.is_empty() {
        return Ok(None);
    }

    // Filter out structs/unions/enums that are part of a typedef
    let mut filtered: Vec<_> = definitions
        .into_iter()
        .filter(|d| !d.is_typedef_child)
        .collect();

    if filtered.is_empty() {
        return Ok(None);
    }

    // Sort by size (smallest first) to get the innermost definition
    filtered.sort_by_key(|d| d.size);

    let def = filtered.into_iter().next().unwrap();
    Ok(Some((def.code, def.start_line, def.def_type)))
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate file exists
    if !args.file_path.exists() {
        anyhow::bail!("File not found: {}", args.file_path.display());
    }

    if args.file_path.is_dir() {
        anyhow::bail!(
            "Expected a file but received a directory: {}",
            args.file_path.display()
        );
    }

    // Determine language
    let lang = args.lang.unwrap_or_else(|| {
        args.file_path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Lang::from_extension)
            .unwrap_or_default()
    });

    match find_innermost_definition(&args.file_path, args.line_number, lang)? {
        Some((code, start_line, def_type)) => {
            if args.show_type {
                println!("# {def_type} starting at line {start_line}");
            }

            // Print with line numbers
            for (i, line) in code.lines().enumerate() {
                println!("{}. {}", start_line + i, line);
            }
        }
        None => {
            eprintln!(
                "No enclosing definition found for line {}",
                args.line_number
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file(content: &str, extension: &str) -> NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(extension)
            .tempfile()
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn test_find_function_definition() {
        let content = r#"
int add(int a, int b) {
    return a + b;
}
"#;
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(&file.path().to_path_buf(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (code, start_line, def_type) = result.unwrap();
        assert_eq!(def_type, "function_definition");
        assert_eq!(start_line, 2);
        assert!(code.contains("int add"));
    }

    #[test]
    fn test_find_struct_definition() {
        let content = r#"
struct Point {
    int x;
    int y;
};
"#;
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(&file.path().to_path_buf(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (_, _, def_type) = result.unwrap();
        assert_eq!(def_type, "struct_specifier");
    }

    #[test]
    fn test_find_typedef() {
        let content = r#"
typedef struct {
    int x;
    int y;
} Point;
"#;
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(&file.path().to_path_buf(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (_, _, def_type) = result.unwrap();
        assert_eq!(def_type, "type_definition");
    }

    #[test]
    fn test_no_definition_found() {
        let content = r#"
// Just a comment
"#;
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(&file.path().to_path_buf(), 2, Lang::C).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_lang_detection() {
        assert!(matches!(Lang::from_extension("c"), Some(Lang::C)));
        assert!(matches!(Lang::from_extension("h"), Some(Lang::C)));
        assert!(matches!(Lang::from_extension("C"), Some(Lang::C)));
        assert!(Lang::from_extension("py").is_none());
        assert!(Lang::from_extension("rs").is_none());
    }
}
