//! Extract code definitions from source files using tree-sitter.
//!
//! This tool parses source code and provides two main features:
//! - Find the innermost enclosing definition for a given line number
//! - List all definitions in a file (outline)
//!
//! Currently supported languages:
//! - C

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use tree_sitter::{Language, Node, Parser as TsParser, Tree};

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
    const fn definition_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &[
                "function_definition",
                "type_definition",      // typedef
                "preproc_def",          // #define
                "preproc_function_def", // #define with parameters
            ],
        }
    }

    /// Get compound types that need body check
    const fn compound_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &["struct_specifier", "union_specifier", "enum_specifier"],
        }
    }

    /// Get body node types for compound types
    const fn body_types(self) -> &'static [&'static str] {
        match self {
            Self::C => &["field_declaration_list", "enumerator_list"],
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
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Find the innermost enclosing definition for a given line number
    Find {
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
    },

    /// List all definitions in a file (outline)
    Outline {
        /// Path to the source file
        file_path: PathBuf,

        /// Programming language (auto-detected from extension if not specified)
        #[arg(short, long, value_enum)]
        lang: Option<Lang>,
    },
}

/// Represents a found definition
#[derive(Debug)]
struct Definition {
    code: String,
    start_line: usize,
    #[allow(dead_code)]
    end_line: usize,
    def_type: String,
    size: usize,
    is_typedef_child: bool,
}

/// Represents an outline entry
#[derive(Debug)]
struct OutlineEntry {
    line: usize,
    end_line: usize,
    signature: String,
    def_type: String,
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

/// Traverse the AST and collect matching definitions for a specific line
fn traverse_for_line(
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
        let end_line = node.end_position().row + 1;
        let size = node.end_byte() - node.start_byte();

        definitions.push(Definition {
            code,
            start_line,
            end_line,
            def_type: node_type.to_string(),
            size,
            is_typedef_child: is_parent_typedef && is_compound_type(node_type, lang),
        });
    }

    // Continue searching children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_for_line(
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

/// Traverse the AST and collect all definitions for outline
fn traverse_for_outline(
    node: Node<'_>,
    source_code: &str,
    depth: usize,
    entries: &mut Vec<OutlineEntry>,
    lang: Lang,
    is_parent_typedef: bool,
) {
    if depth >= MAX_DEFINITION_SEARCH_DEPTH {
        return;
    }

    let node_type = node.kind();
    let mut is_definition = false;
    let mut mark_compound_child = false;
    let mut skip_as_typedef_child = false;

    if is_definition_type(node_type, lang) {
        is_definition = true;
        if node_type == "type_definition" {
            mark_compound_child = true;
        }
    } else if is_compound_type(node_type, lang) && has_body(&node, lang) {
        is_definition = true;
        if is_parent_typedef {
            skip_as_typedef_child = true;
        }
    }

    if is_definition && !skip_as_typedef_child {
        let line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;
        let signature = extract_signature(&node, source_code, lang);

        entries.push(OutlineEntry {
            line,
            end_line,
            signature,
            def_type: node_type.to_string(),
        });
    }

    // Continue searching children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_for_outline(
            child,
            source_code,
            depth + 1,
            entries,
            lang,
            mark_compound_child || is_parent_typedef,
        );
    }
}

/// Extract a compact signature from a definition node
fn extract_signature(node: &Node, source_code: &str, lang: Lang) -> String {
    match lang {
        Lang::C => extract_c_signature(node, source_code),
    }
}

/// Extract signature for C language definitions
fn extract_c_signature(node: &Node, source_code: &str) -> String {
    let node_type = node.kind();

    match node_type {
        "function_definition" => {
            // Extract declarator (function name and parameters)
            if let Some(declarator) = node.child_by_field_name("declarator") {
                let sig = compact_whitespace(&get_node_text(&declarator, source_code));
                // Also get return type
                if let Some(type_node) = node.child_by_field_name("type") {
                    let ret_type = compact_whitespace(&get_node_text(&type_node, source_code));
                    return format!("{ret_type} {sig}");
                }
                return sig;
            }
            get_first_line(node, source_code)
        }
        "type_definition" => extract_typedef_signature(node, source_code),
        "struct_specifier" | "union_specifier" | "enum_specifier" => {
            // Get the keyword and name
            let keyword = match node_type {
                "struct_specifier" => "struct",
                "union_specifier" => "union",
                "enum_specifier" => "enum",
                _ => "",
            };
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = compact_whitespace(&get_node_text(&name_node, source_code));
                return format!("{keyword} {name}");
            }
            format!("{keyword} {{...}}")
        }
        "preproc_def" | "preproc_function_def" => {
            // Get the macro definition line
            get_first_line(node, source_code)
        }
        _ => get_first_line(node, source_code),
    }
}

/// Extract a concise typedef signature
fn extract_typedef_signature(node: &Node, source_code: &str) -> String {
    let type_sig = node
        .child_by_field_name("type")
        .map(|type_node| match type_node.kind() {
            "struct_specifier" | "union_specifier" | "enum_specifier" => {
                extract_c_signature(&type_node, source_code)
            }
            _ => compact_whitespace(&get_node_text(&type_node, source_code)),
        })
        .unwrap_or_default();

    let mut declarators = Vec::new();
    let child_count = node.child_count();
    for i in 0..child_count {
        let Ok(index) = u32::try_from(i) else {
            continue;
        };
        if node.field_name_for_child(index) == Some("declarator") {
            if let Some(child) = node.child(i) {
                let text = compact_whitespace(&get_node_text(&child, source_code));
                if !text.is_empty() {
                    declarators.push(text);
                }
            }
        }
    }

    match (type_sig.is_empty(), declarators.is_empty()) {
        (false, false) => format!("typedef {type_sig} {}", declarators.join(", ")),
        (false, true) => format!("typedef {type_sig}"),
        (true, false) => format!("typedef {}", declarators.join(", ")),
        (true, true) => get_first_line(node, source_code),
    }
}

/// Collapse consecutive whitespace into single spaces
fn compact_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Get text content of a node
fn get_node_text(node: &Node, source_code: &str) -> String {
    source_code
        .get(node.start_byte()..node.end_byte())
        .unwrap_or("")
        .to_string()
}

/// Get the first line of a node's text
fn get_first_line(node: &Node, source_code: &str) -> String {
    let text = get_node_text(node, source_code);
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Format definition type for display
fn format_def_type(def_type: &str) -> &str {
    match def_type {
        "function_definition" => "fn",
        "type_definition" => "typedef",
        "struct_specifier" => "struct",
        "union_specifier" => "union",
        "enum_specifier" => "enum",
        "preproc_def" | "preproc_function_def" => "macro",
        _ => def_type,
    }
}

/// Parse source file and return AST
fn parse_file(file_path: &Path, lang: Lang) -> Result<(String, Tree)> {
    let source_code = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let mut parser = TsParser::new();
    let language = lang.tree_sitter_language();
    parser
        .set_language(&language)
        .context("Failed to set language for parser")?;

    let tree = parser
        .parse(&source_code, None)
        .context("Failed to parse source code")?;

    Ok((source_code, tree))
}

/// Detect language from file path
fn detect_lang(file_path: &Path, explicit_lang: Option<Lang>) -> Lang {
    explicit_lang.unwrap_or_else(|| {
        file_path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Lang::from_extension)
            .unwrap_or_default()
    })
}

/// Validate file path
fn validate_file(file_path: &Path) -> Result<()> {
    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path.display());
    }
    if file_path.is_dir() {
        anyhow::bail!(
            "Expected a file but received a directory: {}",
            file_path.display()
        );
    }
    Ok(())
}

/// Find the innermost definition for a given line number
fn find_innermost_definition(
    file_path: &Path,
    line_number: usize,
    lang: Lang,
) -> Result<Option<(String, usize, String)>> {
    let (source_code, tree) = parse_file(file_path, lang)?;
    let target_row = line_number - 1;

    let mut definitions = Vec::new();

    traverse_for_line(
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

/// List all definitions in a file
fn list_outline(file_path: &Path, lang: Lang) -> Result<Vec<OutlineEntry>> {
    let (source_code, tree) = parse_file(file_path, lang)?;

    let mut entries = Vec::new();

    traverse_for_outline(tree.root_node(), &source_code, 0, &mut entries, lang, false);

    // Sort by line number
    entries.sort_by_key(|e| e.line);

    Ok(entries)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Find {
            file_path,
            line_number,
            lang,
            show_type,
        } => {
            validate_file(&file_path)?;
            let lang = detect_lang(&file_path, lang);

            if let Some((code, start_line, def_type)) =
                find_innermost_definition(&file_path, line_number, lang)?
            {
                if show_type {
                    println!("# {def_type} starting at line {start_line}");
                }

                for (i, line) in code.lines().enumerate() {
                    println!("{}. {}", start_line + i, line);
                }
            } else {
                eprintln!("No enclosing definition found for line {line_number}");
                std::process::exit(1);
            }
        }

        Commands::Outline { file_path, lang } => {
            validate_file(&file_path)?;
            let lang = detect_lang(&file_path, lang);

            let entries = list_outline(&file_path, lang)?;

            if entries.is_empty() {
                eprintln!("No definitions found in {}", file_path.display());
                std::process::exit(1);
            }

            // Calculate line number width for alignment
            let max_line = entries.iter().map(|e| e.end_line).max().unwrap_or(1);
            let line_width = max_line.to_string().len();

            for entry in entries {
                let def_type = format_def_type(&entry.def_type);
                println!(
                    "{:>width$}: [{:<7}] {}",
                    entry.line,
                    def_type,
                    entry.signature,
                    width = line_width
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

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
        let content = r"
int add(int a, int b) {
    return a + b;
}
";
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(file.path(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (code, start_line, def_type) = result.unwrap();
        assert_eq!(def_type, "function_definition");
        assert_eq!(start_line, 2);
        assert!(code.contains("int add"));
    }

    #[test]
    fn test_find_struct_definition() {
        let content = r"
struct Point {
    int x;
    int y;
};
";
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(file.path(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (_, _, def_type) = result.unwrap();
        assert_eq!(def_type, "struct_specifier");
    }

    #[test]
    fn test_find_typedef() {
        let content = r"
typedef struct {
    int x;
    int y;
} Point;
";
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(file.path(), 3, Lang::C).unwrap();
        assert!(result.is_some());
        let (_, _, def_type) = result.unwrap();
        assert_eq!(def_type, "type_definition");
    }

    #[test]
    fn test_no_definition_found() {
        let content = r"
// Just a comment
";
        let file = create_temp_file(content, ".c");
        let result = find_innermost_definition(file.path(), 2, Lang::C).unwrap();
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

    #[test]
    fn test_outline_functions() {
        let content = r"
int add(int a, int b) {
    return a + b;
}

int subtract(int a, int b) {
    return a - b;
}
";
        let file = create_temp_file(content, ".c");
        let entries = list_outline(file.path(), Lang::C).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line, 2);
        assert_eq!(entries[1].line, 6);
        assert!(entries[0].signature.contains("add"));
        assert!(entries[1].signature.contains("subtract"));
    }

    #[test]
    fn test_outline_mixed_definitions() {
        let content = r"
#define MAX_SIZE 100

struct Point {
    int x;
    int y;
};

typedef struct {
    int width;
    int height;
} Rectangle;

int calculate_area(Rectangle* r) {
    return r->width * r->height;
}
";
        let file = create_temp_file(content, ".c");
        let entries = list_outline(file.path(), Lang::C).unwrap();
        assert_eq!(entries.len(), 4); // macro, struct, typedef, function

        let typedef_entry = entries
            .iter()
            .find(|entry| entry.def_type == "type_definition")
            .expect("typedef entry");
        assert!(typedef_entry.signature.starts_with("typedef"));
        assert!(typedef_entry.signature.contains("Rectangle"));
    }

    #[test]
    fn test_format_def_type() {
        assert_eq!(format_def_type("function_definition"), "fn");
        assert_eq!(format_def_type("struct_specifier"), "struct");
        assert_eq!(format_def_type("type_definition"), "typedef");
        assert_eq!(format_def_type("preproc_def"), "macro");
    }
}
