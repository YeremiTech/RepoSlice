use reposlice_core::{
    read_source_text, stable_hash, AnalysisContribution, AnalysisMetadata, Component,
    ComponentKind, Dependency, DependencyKind, Evidence, EvidenceKind, LanguageAnalyzer,
    ScanPolicy, SymbolMetadata, DEFAULT_MAX_SOURCE_SIZE,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhpMethod {
    pub name: String,
    pub visibility: String,
    pub parameter_types: Vec<String>,
    pub calls: Vec<PhpCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhpCall {
    pub receiver: String,
    pub method: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhpType {
    pub component_id: String,
    pub name: String,
    pub qualified_name: String,
    pub namespace: String,
    pub kind: ComponentKind,
    pub file: String,
    pub imports: BTreeMap<String, String>,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub traits: Vec<String>,
    pub methods: Vec<PhpMethod>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhpIndex {
    pub types: Vec<PhpType>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhpAnalysis {
    pub components: Vec<Component>,
    pub dependencies: Vec<Dependency>,
    pub index: PhpIndex,
}

pub struct PhpLanguageAnalyzer;

impl LanguageAnalyzer for PhpLanguageAnalyzer {
    fn id(&self) -> &'static str {
        "php"
    }
    fn supports(&self, path: &Path) -> bool {
        path.extension().and_then(|value| value.to_str()) == Some("php")
    }
    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let parsed = parse_php(root)?;
        let symbols = parsed
            .index
            .types
            .iter()
            .map(|item| SymbolMetadata {
                symbol_id: item.component_id.clone(),
                qualified_name: Some(item.qualified_name.clone()),
                namespace: (!item.namespace.is_empty()).then(|| item.namespace.clone()),
                framework_kind: None,
                attributes: Vec::new(),
                evidence: vec![Evidence {
                    kind: EvidenceKind::Source,
                    source: item.file.clone(),
                    detail: "PHP declaration".into(),
                    confidence: 100,
                }],
                confidence: 100,
            })
            .collect();
        Ok(AnalysisContribution {
            components: parsed.components,
            dependencies: parsed.dependencies,
            metadata: AnalysisMetadata {
                symbols,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Variable,
    String,
    Symbol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    value: String,
}

pub fn parse_php(root: &Path) -> io::Result<PhpAnalysis> {
    let mut files = Vec::new();
    collect_php_files(root, root, &mut files)?;
    files.sort();
    let mut index = PhpIndex::default();
    let mut functions = Vec::new();

    for file in files {
        if fs::metadata(&file)?.len() > DEFAULT_MAX_SOURCE_SIZE {
            continue;
        }
        let content = read_source_text(&file)?;
        let tokens = tokenize(&content);
        parse_file(&file, &tokens, &mut index.types, &mut functions);
    }

    let type_counts = index
        .types
        .iter()
        .fold(BTreeMap::new(), |mut counts, item| {
            *counts
                .entry(normalize_name(&item.qualified_name))
                .or_insert(0usize) += 1;
            counts
        });
    for item in &mut index.types {
        if type_counts
            .get(&normalize_name(&item.qualified_name))
            .is_some_and(|count| *count > 1)
        {
            item.component_id = format!(
                "php:type:{}#{}",
                normalize_name(&item.qualified_name),
                &stable_hash(&item.file)[..12]
            );
        }
    }
    let function_counts = functions.iter().fold(BTreeMap::new(), |mut counts, item| {
        *counts.entry(item.id.clone()).or_insert(0usize) += 1;
        counts
    });
    for item in &mut functions {
        if function_counts
            .get(&item.id)
            .is_some_and(|count| *count > 1)
        {
            item.id = format!("{}#{}", item.id, &stable_hash(&item.file)[..12]);
        }
    }

    index
        .types
        .sort_by(|a, b| a.component_id.cmp(&b.component_id));
    functions.sort_by(|a: &Component, b| a.id.cmp(&b.id));

    let mut components: Vec<Component> = index
        .types
        .iter()
        .map(|item| Component {
            id: item.component_id.clone(),
            name: item.name.clone(),
            kind: item.kind.clone(),
            language: "PHP".into(),
            file: item.file.clone(),
        })
        .collect();
    components.extend(functions);

    let by_qualified: BTreeMap<String, String> = index
        .types
        .iter()
        .filter(|item| type_counts.get(&normalize_name(&item.qualified_name)) == Some(&1))
        .map(|item| {
            (
                normalize_name(&item.qualified_name),
                item.component_id.clone(),
            )
        })
        .collect();
    let mut dependency_keys = BTreeSet::new();
    for item in &index.types {
        for imported in item.imports.values() {
            add_resolved(
                &mut dependency_keys,
                &by_qualified,
                &item.component_id,
                imported,
                DependencyKind::Imports,
            );
        }
        if let Some(parent) = &item.extends {
            add_type_reference(
                &mut dependency_keys,
                &by_qualified,
                item,
                parent,
                DependencyKind::Extends,
            );
        }
        for interface in &item.implements {
            add_type_reference(
                &mut dependency_keys,
                &by_qualified,
                item,
                interface,
                DependencyKind::Implements,
            );
        }
        for trait_name in &item.traits {
            add_type_reference(
                &mut dependency_keys,
                &by_qualified,
                item,
                trait_name,
                DependencyKind::UsesTrait,
            );
        }
        for parameter in item
            .methods
            .iter()
            .flat_map(|method| &method.parameter_types)
        {
            add_type_reference(
                &mut dependency_keys,
                &by_qualified,
                item,
                parameter,
                DependencyKind::Injects,
            );
        }
    }
    let dependencies = dependency_keys
        .into_iter()
        .map(|(source_id, target_id, kind)| Dependency {
            source_id,
            target_id,
            kind,
        })
        .collect();
    Ok(PhpAnalysis {
        components,
        dependencies,
        index,
    })
}

fn add_type_reference(
    keys: &mut BTreeSet<(String, String, DependencyKind)>,
    index: &BTreeMap<String, String>,
    source: &PhpType,
    name: &str,
    kind: DependencyKind,
) {
    let resolved = resolve_type_name(source, name);
    add_resolved(keys, index, &source.component_id, &resolved, kind);
}

fn add_resolved(
    keys: &mut BTreeSet<(String, String, DependencyKind)>,
    index: &BTreeMap<String, String>,
    source: &str,
    qualified: &str,
    kind: DependencyKind,
) {
    if let Some(target) = index.get(&normalize_name(qualified)) {
        if source != target {
            keys.insert((source.to_string(), target.clone(), kind));
        }
    }
}

fn resolve_type_name(source: &PhpType, name: &str) -> String {
    let normalized = name.trim_start_matches('\\');
    if name.starts_with('\\') {
        return normalized.to_string();
    }
    let first = normalized.split('\\').next().unwrap_or(normalized);
    if let Some(imported) = source.imports.get(first) {
        let suffix = normalized.strip_prefix(first).unwrap_or("");
        return format!("{imported}{suffix}");
    }
    if source.namespace.is_empty() {
        normalized.to_string()
    } else {
        format!("{}\\{}", source.namespace, normalized)
    }
}

pub fn resolve_type_reference(source: &PhpType, name: &str) -> String {
    resolve_type_name(source, name)
}

fn normalize_name(value: &str) -> String {
    value.trim_matches('\\').to_ascii_lowercase()
}

fn parse_file(
    file: &Path,
    tokens: &[Token],
    types: &mut Vec<PhpType>,
    functions: &mut Vec<Component>,
) {
    let namespace = statement_name_after(tokens, "namespace").unwrap_or_default();
    let imports = parse_imports(tokens);
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index].value.as_str() {
            "{" => depth += 1,
            "}" => depth = depth.saturating_sub(1),
            "class" | "interface" | "trait" | "enum" if depth == 0 => {
                if let Some((item, next)) = parse_type(file, tokens, index, &namespace, &imports) {
                    types.push(item);
                    index = next;
                    continue;
                }
            }
            "function" if depth == 0 => {
                if let Some(name) = token_value(tokens, index + 1) {
                    if name != "(" {
                        let qualified = qualify(&namespace, name);
                        functions.push(Component {
                            id: format!("php:function:{}", normalize_name(&qualified)),
                            name: name.to_string(),
                            kind: ComponentKind::Function,
                            language: "PHP".into(),
                            file: file.to_string_lossy().into_owned(),
                        });
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
}

fn parse_type(
    file: &Path,
    tokens: &[Token],
    start: usize,
    namespace: &str,
    imports: &BTreeMap<String, String>,
) -> Option<(PhpType, usize)> {
    let keyword = token_value(tokens, start)?;
    let name = token_value(tokens, start + 1)?.to_string();
    if matches!(name.as_str(), "extends" | "implements" | "{") {
        return None;
    }
    let kind = match keyword {
        "interface" => ComponentKind::Interface,
        "trait" => ComponentKind::Trait,
        "enum" => ComponentKind::Enum,
        _ => ComponentKind::Class,
    };
    let qualified_name = qualify(namespace, &name);
    let mut extends = None;
    let mut implements = Vec::new();
    let mut cursor = start + 2;
    while cursor < tokens.len() && token_value(tokens, cursor) != Some("{") {
        match token_value(tokens, cursor) {
            Some("extends") => {
                extends = read_qualified_name(tokens, cursor + 1).map(|(value, next)| {
                    cursor = next;
                    value
                });
            }
            Some("implements") => {
                cursor += 1;
                while cursor < tokens.len() && token_value(tokens, cursor) != Some("{") {
                    if let Some((value, next)) = read_qualified_name(tokens, cursor) {
                        implements.push(value);
                        cursor = next;
                    } else {
                        cursor += 1;
                    }
                    if token_value(tokens, cursor) == Some(",") {
                        cursor += 1;
                    }
                }
                break;
            }
            _ => cursor += 1,
        }
    }
    if token_value(tokens, cursor) != Some("{") {
        return None;
    }
    let end = matching_brace(tokens, cursor)?;
    let (traits, methods) = parse_type_body(tokens, cursor + 1, end);
    Some((
        PhpType {
            component_id: format!("php:type:{}", normalize_name(&qualified_name)),
            name,
            qualified_name,
            namespace: namespace.to_string(),
            kind,
            file: file.to_string_lossy().into_owned(),
            imports: imports.clone(),
            extends,
            implements,
            traits,
            methods,
        },
        end + 1,
    ))
}

fn parse_type_body(tokens: &[Token], start: usize, end: usize) -> (Vec<String>, Vec<PhpMethod>) {
    let mut traits = Vec::new();
    let mut methods = Vec::new();
    let mut depth = 0usize;
    let mut index = start;
    let mut visibility = "public".to_string();
    while index < end {
        match token_value(tokens, index) {
            Some("{") => depth += 1,
            Some("}") => depth = depth.saturating_sub(1),
            Some("public" | "protected" | "private") if depth == 0 => {
                visibility = tokens[index].value.clone()
            }
            Some("use") if depth == 0 => {
                index += 1;
                while index < end && token_value(tokens, index) != Some(";") {
                    if let Some((name, next)) = read_qualified_name(tokens, index) {
                        traits.push(name);
                        index = next;
                    } else {
                        index += 1;
                    }
                    if token_value(tokens, index) == Some(",") {
                        index += 1;
                    }
                }
            }
            Some("function") if depth == 0 => {
                if let Some((method, next)) = parse_method(tokens, index, &visibility) {
                    methods.push(method);
                    index = next;
                    visibility = "public".into();
                    continue;
                }
            }
            _ => {}
        }
        index += 1;
    }
    (traits, methods)
}

fn parse_method(tokens: &[Token], start: usize, visibility: &str) -> Option<(PhpMethod, usize)> {
    let mut cursor = start + 1;
    if token_value(tokens, cursor) == Some("&") {
        cursor += 1;
    }
    let name = token_value(tokens, cursor)?.to_string();
    cursor += 1;
    if token_value(tokens, cursor) != Some("(") {
        return None;
    }
    let params_end = matching_symbol(tokens, cursor, "(", ")")?;
    let parameter_types = parse_parameter_types(&tokens[cursor + 1..params_end]);
    cursor = params_end + 1;
    while cursor < tokens.len() && !matches!(token_value(tokens, cursor), Some("{" | ";")) {
        cursor += 1;
    }
    let mut calls = Vec::new();
    let next = if token_value(tokens, cursor) == Some("{") {
        let body_end = matching_brace(tokens, cursor)?;
        calls = parse_calls(&tokens[cursor + 1..body_end]);
        body_end + 1
    } else {
        cursor + 1
    };
    Some((
        PhpMethod {
            name,
            visibility: visibility.to_string(),
            parameter_types,
            calls,
        },
        next,
    ))
}

fn parse_parameter_types(tokens: &[Token]) -> Vec<String> {
    let mut result = Vec::new();
    let mut segment = Vec::new();
    for token in tokens.iter().chain(std::iter::once(&Token {
        kind: TokenKind::Symbol,
        value: ",".into(),
    })) {
        if token.value == "," {
            if let Some(variable) = segment
                .iter()
                .position(|item: &Token| item.kind == TokenKind::Variable)
            {
                let value = segment[..variable]
                    .iter()
                    .filter(|item| item.value != "?" && item.value != "|")
                    .map(|item| item.value.as_str())
                    .collect::<String>();
                if !value.is_empty() && !is_builtin(&value) {
                    result.push(value);
                }
            }
            segment.clear();
        } else {
            segment.push(token.clone());
        }
    }
    result
}

fn parse_calls(tokens: &[Token]) -> Vec<PhpCall> {
    let mut calls = Vec::new();
    let mut index = 0;
    while index + 3 < tokens.len() {
        if matches!(
            tokens[index].kind,
            TokenKind::Variable | TokenKind::Identifier
        ) && matches!(token_value(tokens, index + 1), Some("->" | "::"))
            && tokens[index + 2].kind == TokenKind::Identifier
            && token_value(tokens, index + 3) == Some("(")
        {
            if let Some(end) = matching_symbol(tokens, index + 3, "(", ")") {
                calls.push(PhpCall {
                    receiver: tokens[index].value.clone(),
                    method: tokens[index + 2].value.clone(),
                    arguments: split_arguments(&tokens[index + 4..end]),
                });
                index = end;
            }
        }
        index += 1;
    }
    calls
}

fn split_arguments(tokens: &[Token]) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for token in tokens {
        match token.value.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            "," if depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
                continue;
            }
            _ => {}
        }
        if !current.is_empty() && token.kind != TokenKind::Symbol {
            current.push(' ');
        }
        current.push_str(&token.value);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn parse_imports(tokens: &[Token]) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < tokens.len() {
        match token_value(tokens, index) {
            Some("{") => depth += 1,
            Some("}") => depth = depth.saturating_sub(1),
            Some("use") if depth == 0 => {
                if matches!(token_value(tokens, index + 1), Some("function" | "const")) {
                    index += 1;
                    continue;
                }
                if let Some((name, next)) = read_qualified_name(tokens, index + 1) {
                    let mut alias = name.rsplit('\\').next().unwrap_or(&name).to_string();
                    let mut cursor = next;
                    if token_value(tokens, cursor) == Some("as") {
                        if let Some(value) = token_value(tokens, cursor + 1) {
                            alias = value.to_string();
                        }
                        cursor += 2;
                    }
                    result.insert(alias, name.trim_start_matches('\\').to_string());
                    index = cursor;
                }
            }
            _ => {}
        }
        index += 1;
    }
    result
}

fn statement_name_after(tokens: &[Token], keyword: &str) -> Option<String> {
    let index = tokens.iter().position(|token| token.value == keyword)?;
    read_qualified_name(tokens, index + 1).map(|(value, _)| value.trim_matches('\\').to_string())
}

fn read_qualified_name(tokens: &[Token], start: usize) -> Option<(String, usize)> {
    let mut cursor = start;
    let mut value = String::new();
    while cursor < tokens.len() {
        let token = &tokens[cursor];
        if token.kind == TokenKind::Identifier || token.value == "\\" {
            value.push_str(&token.value);
            cursor += 1;
        } else {
            break;
        }
    }
    (!value.is_empty()).then_some((value, cursor))
}

fn qualify(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

fn matching_brace(tokens: &[Token], start: usize) -> Option<usize> {
    matching_symbol(tokens, start, "{", "}")
}

fn matching_symbol(tokens: &[Token], start: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.value == open {
            depth += 1;
        }
        if token.value == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

fn token_value(tokens: &[Token], index: usize) -> Option<&str> {
    tokens.get(index).map(|token| token.value.as_str())
}

fn is_builtin(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "string"
            | "int"
            | "float"
            | "bool"
            | "array"
            | "callable"
            | "iterable"
            | "object"
            | "mixed"
            | "self"
            | "static"
            | "parent"
            | "null"
    )
}

fn tokenize(content: &str) -> Vec<Token> {
    let chars: Vec<char> = content.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if current.is_whitespace() {
            index += 1;
            continue;
        }
        if current == '/' && next == Some('/') || current == '#' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if current == '/' && next == Some('*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }
        if current == '\'' || current == '"' {
            let quote = current;
            index += 1;
            let mut value = String::new();
            let mut escaped = false;
            while index < chars.len() {
                let character = chars[index];
                index += 1;
                if escaped {
                    value.push(character);
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == quote {
                    break;
                } else {
                    value.push(character);
                }
            }
            tokens.push(Token {
                kind: TokenKind::String,
                value,
            });
            continue;
        }
        if current == '$' {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Variable,
                value: chars[start..index].iter().collect(),
            });
            continue;
        }
        if current.is_alphabetic() || current == '_' || current as u32 >= 128 {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_alphanumeric()
                    || chars[index] == '_'
                    || chars[index] as u32 >= 128)
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Identifier,
                value: chars[start..index].iter().collect(),
            });
            continue;
        }
        let pair = next.map(|value| format!("{current}{value}"));
        if matches!(pair.as_deref(), Some("->" | "::" | "=>" | "??")) {
            tokens.push(Token {
                kind: TokenKind::Symbol,
                value: pair.unwrap(),
            });
            index += 2;
        } else {
            tokens.push(Token {
                kind: TokenKind::Symbol,
                value: current.to_string(),
            });
            index += 1;
        }
    }
    tokens
}

pub fn php_syntax_balanced(content: &str) -> bool {
    let tokens = tokenize(content);
    let mut stack = Vec::new();
    for token in tokens {
        match token.value.as_str() {
            "(" | "[" | "{" => stack.push(token.value),
            ")" | "]" | "}" => {
                let expected = match token.value.as_str() {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                if stack.pop().as_deref() != Some(expected) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}

fn collect_php_files(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if excluded(root, path) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
            ) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if is_php_source(path) {
            files.push(path.to_path_buf());
        }
    } else if metadata.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
                ) =>
            {
                return Ok(())
            }
            Err(error) => return Err(error),
        };
        for entry in entries.flatten() {
            collect_php_files(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

fn is_php_source(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("php")
        && !path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.ends_with(".blade.php"))
}

fn excluded(root: &Path, path: &Path) -> bool {
    ScanPolicy::default().is_excluded_from(root, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_types_without_comment_or_string_false_positives() {
        let tokens = tokenize(
            "<?php /* class Fake {} */ namespace App\\Models; class User extends Model {}",
        );
        let values: Vec<&str> = tokens.iter().map(|token| token.value.as_str()).collect();
        assert!(!values.contains(&"Fake"));
        assert!(values.contains(&"User"));
    }

    #[test]
    fn excludes_blade_templates_from_php_semantic_analysis() {
        assert!(!is_php_source(Path::new(
            "resources/views/welcome.blade.php"
        )));
        assert!(is_php_source(Path::new("app/Models/User.php")));
    }

    #[test]
    fn reads_constructor_types_and_method_calls() {
        let tokens = tokenize("class A { public function __construct(Service $service) {} public function user(){ return $this->belongsTo(User::class); } }");
        let mut types = Vec::new();
        parse_file(Path::new("A.php"), &tokens, &mut types, &mut Vec::new());
        assert_eq!(types[0].methods[0].parameter_types, vec!["Service"]);
        assert_eq!(types[0].methods[1].calls[0].method, "belongsTo");
    }
}
