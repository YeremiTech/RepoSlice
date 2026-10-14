use reposlice_core::{
    read_source_text, AnalysisContribution, Component, ComponentKind, Dependency, DependencyKind,
    LanguageAnalyzer, ScanPolicy, DEFAULT_MAX_SOURCE_SIZE,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct PolyglotLanguageAnalyzer;

impl LanguageAnalyzer for PolyglotLanguageAnalyzer {
    fn id(&self) -> &'static str {
        "polyglot"
    }

    fn supports(&self, path: &Path) -> bool {
        language(path).is_some()
    }

    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let parsed = parse_polyglot(root)?;
        Ok(AnalysisContribution {
            components: parsed.components,
            dependencies: parsed.dependencies,
            ..Default::default()
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct PolyglotAnalysis {
    pub components: Vec<Component>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Clone, Debug)]
struct SourceUnit {
    file: PathBuf,
    language: &'static str,
    module_id: String,
    module_key: String,
    namespace: Option<String>,
    imports: Vec<String>,
}

pub fn parse_polyglot(root: &Path) -> io::Result<PolyglotAnalysis> {
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    let go_module = read_go_module(root);
    let mut units = Vec::new();
    let mut components = Vec::new();

    for file in files {
        let Some(language) = language(&file) else {
            continue;
        };
        let content = read_source_text(&file)?;
        let relative = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let module_key = module_key(&relative);
        let module_id = format!(
            "{}:module:{}",
            language.to_ascii_lowercase().replace('#', "sharp"),
            stable_hash(&relative)
        );
        components.push(Component {
            id: module_id.clone(),
            name: file
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("module")
                .into(),
            kind: ComponentKind::Module,
            language: language.into(),
            file: file.to_string_lossy().into_owned(),
        });
        let namespace = namespace_of(language, &content);
        for (kind, name) in declarations(language, &content) {
            components.push(Component {
                id: format!(
                    "{}:{}:{}:{}",
                    language.to_ascii_lowercase().replace('#', "sharp"),
                    kind.as_str(),
                    stable_hash(&relative),
                    name
                ),
                name,
                kind,
                language: language.into(),
                file: file.to_string_lossy().into_owned(),
            });
        }
        units.push(SourceUnit {
            file,
            language,
            module_id,
            module_key,
            namespace,
            imports: imports(language, &content, go_module.as_deref()),
        });
    }

    let mut aliases = BTreeMap::new();
    for unit in &units {
        aliases.insert(unit.module_key.clone(), unit.module_id.clone());
        if let Some(namespace) = &unit.namespace {
            aliases.insert(namespace.clone(), unit.module_id.clone());
        }
    }
    let mut keys = BTreeSet::new();
    for unit in &units {
        for import in &unit.imports {
            if let Some(target) = resolve_import(unit, import, &aliases) {
                if target != unit.module_id {
                    keys.insert((unit.module_id.clone(), target, DependencyKind::Imports));
                }
            }
        }
        for component in components.iter().filter(|component| {
            component.file == unit.file.to_string_lossy() && component.id != unit.module_id
        }) {
            keys.insert((
                component.id.clone(),
                unit.module_id.clone(),
                DependencyKind::Requires,
            ));
        }
    }
    components.sort_by(|left, right| left.id.cmp(&right.id));
    components.dedup_by(|left, right| left.id == right.id);
    let dependencies = keys
        .into_iter()
        .map(|(source_id, target_id, kind)| Dependency {
            source_id,
            target_id,
            kind,
        })
        .collect();
    Ok(PolyglotAnalysis {
        components,
        dependencies,
    })
}

fn declarations(language: &str, content: &str) -> Vec<(ComponentKind, String)> {
    let mut result = Vec::new();
    for raw in content.lines() {
        let line = raw.trim_start();
        let candidates: &[(&str, ComponentKind)] = match language {
            "Python" => &[
                ("class ", ComponentKind::Class),
                ("def ", ComponentKind::Function),
                ("async def ", ComponentKind::Function),
            ],
            "Ruby" => &[
                ("class ", ComponentKind::Class),
                ("module ", ComponentKind::Module),
                ("def ", ComponentKind::Function),
            ],
            "Go" => &[
                ("type ", ComponentKind::Class),
                ("func ", ComponentKind::Function),
            ],
            "Rust" => &[
                ("struct ", ComponentKind::Class),
                ("enum ", ComponentKind::Enum),
                ("trait ", ComponentKind::Trait),
                ("fn ", ComponentKind::Function),
                ("pub fn ", ComponentKind::Function),
                ("async fn ", ComponentKind::Function),
                ("pub async fn ", ComponentKind::Function),
            ],
            "C#" => &[
                ("class ", ComponentKind::Class),
                ("interface ", ComponentKind::Interface),
                ("record ", ComponentKind::Record),
                ("enum ", ComponentKind::Enum),
            ],
            "Kotlin" => &[
                ("class ", ComponentKind::Class),
                ("data class ", ComponentKind::Record),
                ("interface ", ComponentKind::Interface),
                ("object ", ComponentKind::Module),
                ("fun ", ComponentKind::Function),
            ],
            "Elixir" => &[
                ("defmodule ", ComponentKind::Module),
                ("def ", ComponentKind::Function),
                ("defp ", ComponentKind::Function),
            ],
            _ => &[
                ("class ", ComponentKind::Class),
                ("function ", ComponentKind::Function),
                ("export function ", ComponentKind::Function),
                ("export default function ", ComponentKind::Function),
            ],
        };
        for (marker, kind) in candidates {
            if let Some(position) = keyword_position(line, marker.trim()) {
                let mut tail = line[position + marker.trim().len()..].trim_start();
                if language == "Go" && marker.trim() == "func" && tail.starts_with('(') {
                    if let Some(end) = matching_delimiter(tail, '(', ')') {
                        tail = tail[end + 1..].trim_start();
                    }
                }
                let name = tail
                    .trim_start_matches(['*', '&'])
                    .split(|character: char| {
                        !(character.is_alphanumeric() || character == '_' || character == '$')
                    })
                    .next()
                    .unwrap_or("");
                if !name.is_empty() {
                    result.push((kind.clone(), name.into()));
                }
                break;
            }
        }
    }
    result
}

fn keyword_position(line: &str, keyword: &str) -> Option<usize> {
    let position = line.find(keyword)?;
    let before = line[..position].chars().next_back();
    let after = line[position + keyword.len()..].chars().next();
    if before.is_none_or(|value| !is_identifier(value))
        && after.is_none_or(|value| value.is_whitespace())
    {
        Some(position)
    } else {
        None
    }
}

fn imports(language: &str, content: &str, go_module: Option<&str>) -> Vec<String> {
    let mut result = Vec::new();
    for raw in content.lines() {
        let line = raw.trim().trim_end_matches(';');
        match language {
            "Python" => {
                if let Some(value) = line
                    .strip_prefix("from ")
                    .and_then(|value| value.split_once(" import ").map(|pair| pair.0))
                {
                    result.push(value.into());
                } else if let Some(value) = line.strip_prefix("import ") {
                    result.extend(
                        value
                            .split(',')
                            .map(|value| value.split_whitespace().next().unwrap_or("").into()),
                    );
                }
            }
            "Ruby" => {
                if let Some(value) = quoted_after(line, "require_relative") {
                    result.push(value);
                }
            }
            "Rust" => {
                if let Some(value) = line.strip_prefix("mod ") {
                    result.push(value.trim().into());
                }
            }
            "C#" => {
                if let Some(value) = line.strip_prefix("using ") {
                    result.push(value.trim().into());
                }
            }
            "Kotlin" => {
                if let Some(value) = line.strip_prefix("import ") {
                    result.push(value.split_whitespace().next().unwrap_or("").into());
                }
            }
            "Elixir" => {
                if let Some(value) = line
                    .strip_prefix("alias ")
                    .or_else(|| line.strip_prefix("import "))
                {
                    result.push(value.split([',', ' ']).next().unwrap_or("").into());
                }
            }
            "JavaScript" => {
                if (line.starts_with("import ") || line.starts_with("export "))
                    && line.contains(" from ")
                {
                    if let Some(value) = last_quoted(line) {
                        result.push(value);
                    }
                }
            }
            "Go" => {
                if let Some(value) = quoted_value(line) {
                    result.push(
                        go_module
                            .and_then(|module| value.strip_prefix(&format!("{module}/")))
                            .unwrap_or(&value)
                            .into(),
                    );
                }
            }
            "Vue" | "Svelte" | "Astro" => {
                if (line.starts_with("import ") || line.starts_with("export "))
                    && line.contains(" from ")
                {
                    if let Some(value) = last_quoted(line) {
                        result.push(value);
                    }
                }
            }
            "Razor" => {
                if let Some(value) = line.strip_prefix("@using ") {
                    result.push(value.trim().into());
                }
            }
            _ => {}
        }
    }
    result
}

fn resolve_import(
    unit: &SourceUnit,
    import: &str,
    aliases: &BTreeMap<String, String>,
) -> Option<String> {
    let raw = import.trim_matches(['\'', '"']).to_string();
    if matches!(unit.language, "JavaScript" | "Vue" | "Svelte" | "Astro") && raw.starts_with('.') {
        let parent = Path::new(&unit.module_key)
            .parent()
            .unwrap_or(Path::new(""));
        let joined = normalize_components(&parent.join(&raw));
        return lookup_alias(&joined, aliases);
    }
    if unit.language == "Ruby" {
        let parent = Path::new(&unit.module_key)
            .parent()
            .unwrap_or(Path::new(""));
        return lookup_alias(&normalize_components(&parent.join(raw)), aliases);
    }
    lookup_alias(raw.trim_start_matches('/'), aliases)
        .or_else(|| lookup_alias(&raw.replace('.', "/"), aliases))
        .or_else(|| {
            let mut qualified = raw.as_str();
            while let Some((parent, _)) = qualified.rsplit_once(['.', ':']) {
                if let Some(value) = lookup_alias(parent, aliases) {
                    return Some(value);
                }
                qualified = parent;
            }
            None
        })
}

fn lookup_alias(value: &str, aliases: &BTreeMap<String, String>) -> Option<String> {
    aliases
        .get(value)
        .or_else(|| aliases.get(&format!("{value}/index")))
        .or_else(|| {
            aliases
                .iter()
                .find(|(key, _)| key.ends_with(&format!("/{value}")) || *key == value)
                .map(|(_, id)| id)
        })
        .cloned()
}

fn module_key(relative: &str) -> String {
    let without_extension = relative
        .rsplit_once('.')
        .map(|pair| pair.0)
        .unwrap_or(relative);
    without_extension
        .trim_end_matches("/__init__")
        .trim_end_matches("/mod")
        .to_string()
}

fn namespace_of(language: &str, content: &str) -> Option<String> {
    let marker = match language {
        "C#" => "namespace ",
        "Kotlin" => "package ",
        "Elixir" => "defmodule ",
        _ => return None,
    };
    content.lines().find_map(|line| {
        line.trim()
            .strip_prefix(marker)
            .map(|value| {
                value
                    .trim_end_matches([';', '{'])
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    })
}

fn language(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("js" | "jsx" | "mjs" | "cjs") => Some("JavaScript"),
        Some("py") => Some("Python"),
        Some("cs") => Some("C#"),
        Some("rb") => Some("Ruby"),
        Some("go") => Some("Go"),
        Some("rs") => Some("Rust"),
        Some("kt" | "kts") => Some("Kotlin"),
        Some("ex" | "exs") => Some("Elixir"),
        Some("vue") => Some("Vue"),
        Some("svelte") => Some("Svelte"),
        Some("astro") => Some("Astro"),
        Some("razor") => Some("Razor"),
        _ => None,
    }
}

fn read_go_module(root: &Path) -> Option<String> {
    read_source_text(root.join("go.mod"))
        .ok()?
        .lines()
        .find_map(|line| line.trim().strip_prefix("module ").map(str::to_string))
}

fn quoted_after(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .and_then(|value| quoted_value(value.trim()))
}
fn last_quoted(line: &str) -> Option<String> {
    let start = line.rfind(['\'', '"'])?;
    let quote = line.as_bytes()[start] as char;
    let prior = line[..start].rfind(quote)?;
    Some(line[prior + 1..start].into())
}
fn quoted_value(line: &str) -> Option<String> {
    let start = line.find(['\'', '"'])?;
    let quote = line.as_bytes()[start] as char;
    let end = line[start + 1..].find(quote)? + start + 1;
    Some(line[start + 1..end].into())
}
fn is_identifier(value: char) -> bool {
    value.is_alphanumeric() || matches!(value, '_' | '$')
}

fn matching_delimiter(value: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, character) in value.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}
fn normalize_components(path: &Path) -> String {
    let mut values = Vec::new();
    for item in path.components() {
        let value = item.as_os_str().to_string_lossy();
        if value == ".." {
            values.pop();
        } else if value != "." {
            values.push(value.into_owned());
        }
    }
    values.join("/")
}
fn stable_hash(value: &str) -> String {
    let mut hash = 14695981039346656037u64;
    for byte in value.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    if ScanPolicy::default().is_excluded_from(root, path) {
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
        if metadata.len() <= DEFAULT_MAX_SOURCE_SIZE && language(path).is_some() {
            files.push(path.into());
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
            collect(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_declarations() {
        assert!(
            declarations("Python", "class User:\n    def save(self): pass")
                .iter()
                .any(|(_, name)| name == "User")
        );
        assert!(declarations("Rust", "pub fn health() {}")
            .iter()
            .any(|(_, name)| name == "health"));
    }
    #[test]
    fn normalizes_relative_modules() {
        assert_eq!(
            normalize_components(Path::new("src/pages/../api")),
            "src/api"
        );
    }
}
