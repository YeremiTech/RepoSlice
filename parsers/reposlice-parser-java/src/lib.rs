use reposlice_core::{
    read_source_text, stable_hash, AnalysisContribution, AnalysisMetadata, Component,
    ComponentKind, Dependency, DependencyKind, Evidence, EvidenceKind, LanguageAnalyzer,
    ScanPolicy, SymbolMetadata, DEFAULT_MAX_SOURCE_SIZE,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct JavaSource {
    component: Component,
    package_name: String,
    qualified_name: String,
    imports: Vec<String>,
    static_imports: Vec<String>,
    code_content: String,
}

pub struct JavaLanguageAnalyzer;

impl LanguageAnalyzer for JavaLanguageAnalyzer {
    fn id(&self) -> &'static str {
        "java"
    }
    fn supports(&self, path: &Path) -> bool {
        path.extension().and_then(|value| value.to_str()) == Some("java")
    }
    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let (components, dependencies) = parse_java(root)?;
        let symbols = components
            .iter()
            .map(|component| {
                let qualified_name = component
                    .id
                    .strip_prefix("java:")
                    .map(|value| value.split('#').next().unwrap_or(value).to_string());
                let namespace = qualified_name.as_ref().and_then(|value| {
                    value
                        .rsplit_once('.')
                        .map(|(namespace, _)| namespace.to_string())
                });
                SymbolMetadata {
                    symbol_id: component.id.clone(),
                    qualified_name,
                    namespace,
                    framework_kind: None,
                    attributes: Vec::new(),
                    evidence: vec![Evidence {
                        kind: EvidenceKind::Source,
                        source: component.file.clone(),
                        detail: "Java type declaration".into(),
                        confidence: 100,
                    }],
                    confidence: 100,
                }
            })
            .collect();
        Ok(AnalysisContribution {
            components,
            dependencies,
            metadata: AnalysisMetadata {
                symbols,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

pub fn parse_java(root: &Path) -> io::Result<(Vec<Component>, Vec<Dependency>)> {
    let files = collect_java_files(root)?;
    let mut sources = Vec::new();

    for file in files {
        let content = read_source_text(&file)?;
        let clean_content = strip_comments_and_literals(&content);
        let Some((kind, name)) = primary_type(&clean_content) else {
            continue;
        };
        let package_name = package_name(&clean_content).unwrap_or_default();
        let qualified_name = if package_name.is_empty() {
            name.clone()
        } else {
            format!("{package_name}.{name}")
        };
        let (imports, static_imports) = imports(&clean_content);
        let code_content = code_content(&clean_content);

        sources.push(JavaSource {
            component: Component {
                id: format!("java:{qualified_name}"),
                name,
                kind,
                language: "Java".to_string(),
                file: file.to_string_lossy().to_string(),
            },
            package_name,
            qualified_name,
            imports,
            static_imports,
            code_content,
        });
    }

    let qualified_counts = sources.iter().fold(BTreeMap::new(), |mut counts, source| {
        *counts
            .entry(source.qualified_name.clone())
            .or_insert(0usize) += 1;
        counts
    });
    for source in &mut sources {
        if qualified_counts
            .get(&source.qualified_name)
            .is_some_and(|count| *count > 1)
        {
            source.component.id = format!(
                "java:{}#{}",
                source.qualified_name,
                &stable_hash(&source.component.file)[..12]
            );
        }
    }

    let by_qualified: BTreeMap<String, String> = sources
        .iter()
        .filter(|source| qualified_counts.get(&source.qualified_name) == Some(&1))
        .map(|source| (source.qualified_name.clone(), source.component.id.clone()))
        .collect();

    let by_package: BTreeMap<String, Vec<(String, String)>> = {
        let mut index: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for source in &sources {
            index
                .entry(source.package_name.clone())
                .or_default()
                .push((source.component.name.clone(), source.component.id.clone()));
        }
        for candidates in index.values_mut() {
            let counts = candidates
                .iter()
                .fold(BTreeMap::new(), |mut counts, (name, _)| {
                    *counts.entry(name.clone()).or_insert(0usize) += 1;
                    counts
                });
            candidates.retain(|(name, _)| counts.get(name) == Some(&1));
        }
        index
    };

    let mut dependency_keys = BTreeSet::new();

    for source in &sources {
        for import_path in &source.imports {
            if let Some(target_id) = by_qualified.get(import_path) {
                insert_dependency(
                    &mut dependency_keys,
                    &source.component.id,
                    target_id,
                    "imports",
                );
                continue;
            }

            if let Some(package) = import_path.strip_suffix(".*") {
                if let Some(candidates) = by_package.get(package) {
                    for (name, target_id) in candidates {
                        if contains_identifier(&source.code_content, name) {
                            insert_dependency(
                                &mut dependency_keys,
                                &source.component.id,
                                target_id,
                                "imports",
                            );
                        }
                    }
                }
            }
        }

        for import_path in &source.static_imports {
            if let Some(target_id) = resolve_qualified_owner(import_path, &by_qualified) {
                insert_dependency(
                    &mut dependency_keys,
                    &source.component.id,
                    target_id,
                    "imports",
                );
            }
        }

        if let Some(candidates) = by_package.get(&source.package_name) {
            for (name, target_id) in candidates {
                if target_id == &source.component.id {
                    continue;
                }

                if contains_identifier(&source.code_content, name) {
                    insert_dependency(
                        &mut dependency_keys,
                        &source.component.id,
                        target_id,
                        "references",
                    );
                }
            }
        }

        for reference in qualified_references(&source.code_content) {
            let Some(target_id) = resolve_qualified_owner(&reference, &by_qualified) else {
                continue;
            };
            if target_id != &source.component.id {
                insert_dependency(
                    &mut dependency_keys,
                    &source.component.id,
                    target_id,
                    "references",
                );
            }
        }
    }

    let dependencies = dependency_keys
        .into_iter()
        .map(|(source_id, target_id, kind)| Dependency {
            source_id,
            target_id,
            kind: if kind == "imports" {
                DependencyKind::Imports
            } else {
                DependencyKind::References
            },
        })
        .collect();

    let components = sources.into_iter().map(|source| source.component).collect();

    Ok((components, dependencies))
}

pub fn collect_java_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn insert_dependency(
    dependencies: &mut BTreeSet<(String, String, String)>,
    source_id: &str,
    target_id: &str,
    kind: &str,
) {
    if source_id != target_id {
        dependencies.insert((
            source_id.to_string(),
            target_id.to_string(),
            kind.to_string(),
        ));
    }
}

fn package_name(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("package ")
            .map(|value| value.trim_end_matches(';').trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn imports(content: &str) -> (Vec<String>, Vec<String>) {
    let mut normal = Vec::new();
    let mut static_imports = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("import ") else {
            continue;
        };
        let value = value.trim_end_matches(';').trim();

        if let Some(static_value) = value.strip_prefix("static ") {
            static_imports.push(static_value.trim().to_string());
        } else if !value.is_empty() {
            normal.push(value.to_string());
        }
    }

    (normal, static_imports)
}

fn code_content(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let value = line.trim_start();
            !value.starts_with("package ") && !value.starts_with("import ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn primary_type(content: &str) -> Option<(ComponentKind, String)> {
    for line in content.lines() {
        let normalized = line.replace(['{', '(', ')', '<', '>'], " ");
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        let mappings = [
            ("class", ComponentKind::Class),
            ("interface", ComponentKind::Interface),
            ("record", ComponentKind::Record),
            ("enum", ComponentKind::Enum),
        ];

        for (keyword, kind) in mappings {
            if let Some(index) = tokens.iter().position(|token| *token == keyword) {
                if let Some(name) = tokens.get(index + 1) {
                    let clean = name
                        .trim_matches(|character: char| {
                            !character.is_alphanumeric() && character != '_' && character != '$'
                        })
                        .to_string();
                    if !clean.is_empty() {
                        return Some((kind, clean));
                    }
                }
            }
        }
    }

    None
}

fn contains_identifier(content: &str, value: &str) -> bool {
    content
        .split(|character: char| {
            !character.is_alphanumeric() && character != '_' && character != '$'
        })
        .any(|token| token == value)
}

fn qualified_references(content: &str) -> BTreeSet<String> {
    content
        .split(|character: char| {
            !(character.is_alphanumeric()
                || character == '_'
                || character == '$'
                || character == '.')
        })
        .map(|value| value.trim_matches('.'))
        .filter(|value| value.contains('.'))
        .map(str::to_string)
        .collect()
}

fn resolve_qualified_owner<'a>(
    value: &str,
    by_qualified: &'a BTreeMap<String, String>,
) -> Option<&'a String> {
    let mut candidate = value.trim_end_matches('.');
    loop {
        if let Some(target) = by_qualified.get(candidate) {
            return Some(target);
        }
        candidate = candidate.rsplit_once('.')?.0;
    }
}

fn strip_comments_and_literals(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut output = String::with_capacity(chars.len());
    let mut index = 0;
    let mut block_comment = false;
    let mut line_comment = false;
    let mut string_literal = false;
    let mut char_literal = false;
    let mut escaped = false;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if line_comment {
            if current == '\n' {
                line_comment = false;
                output.push('\n');
            } else {
                output.push(' ');
            }
            index += 1;
            continue;
        }

        if block_comment {
            if current == '*' && next == Some('/') {
                output.push(' ');
                output.push(' ');
                block_comment = false;
                index += 2;
            } else {
                output.push(if current == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }

        if string_literal {
            output.push(if current == '\n' { '\n' } else { ' ' });
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                string_literal = false;
            }
            index += 1;
            continue;
        }

        if char_literal {
            output.push(if current == '\n' { '\n' } else { ' ' });
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '\'' {
                char_literal = false;
            }
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            output.push(' ');
            output.push(' ');
            line_comment = true;
            index += 2;
            continue;
        }

        if current == '/' && next == Some('*') {
            output.push(' ');
            output.push(' ');
            block_comment = true;
            index += 2;
            continue;
        }

        if current == '"' {
            output.push(' ');
            string_literal = true;
            index += 1;
            continue;
        }

        if current == '\'' {
            output.push(' ');
            char_literal = true;
            index += 1;
            continue;
        }

        output.push(current);
        index += 1;
    }

    output
}

fn visit(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if skip(root, path) {
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
        if path.extension().and_then(|value| value.to_str()) == Some("java")
            && metadata.len() <= DEFAULT_MAX_SOURCE_SIZE
        {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if metadata.is_dir() {
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
            visit(root, &entry.path(), files)?;
        }
    }

    Ok(())
}

fn skip(root: &Path, path: &Path) -> bool {
    ScanPolicy::default().is_excluded_from(root, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_comment_and_string_content() {
        let value = strip_comments_and_literals(
            "class A { String x = \"FakeType\"; // FakeType\n/* FakeType */ RealType y; }",
        );
        assert!(!contains_identifier(&value, "FakeType"));
        assert!(contains_identifier(&value, "RealType"));
    }

    #[test]
    fn extracts_static_and_normal_imports() {
        let value = "import a.b.User;\nimport static a.b.Permission.READ;";
        let (normal, static_imports) = imports(value);
        assert_eq!(normal, vec!["a.b.User"]);
        assert_eq!(static_imports, vec!["a.b.Permission.READ"]);
    }
}
