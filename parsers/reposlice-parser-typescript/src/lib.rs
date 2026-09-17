use reposlice_core::{
    read_source_text, AnalysisContribution, Component, ComponentKind, Dependency, DependencyKind,
    HttpCall, LanguageAnalyzer, ScanPolicy, DEFAULT_MAX_SOURCE_SIZE,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct TypeScriptLanguageAnalyzer;

impl LanguageAnalyzer for TypeScriptLanguageAnalyzer {
    fn id(&self) -> &'static str {
        "typescript"
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("ts") | Some("tsx")
        )
    }

    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let (components, dependencies) = parse_typescript(root)?;
        Ok(AnalysisContribution {
            components,
            dependencies,
            ..Default::default()
        })
    }
}

#[derive(Clone, Debug)]
struct AliasRule {
    pattern: String,
    targets: Vec<String>,
}

#[derive(Clone, Debug)]
struct TypeScriptConfig {
    base_dir: PathBuf,
    aliases: Vec<AliasRule>,
}

pub fn parse_typescript(root: &Path) -> io::Result<(Vec<Component>, Vec<Dependency>)> {
    let files = collect_typescript_files(root)?;
    let canonical_root = canonical_or_original(root);
    let config = load_typescript_config(root);
    let mut components = Vec::new();

    for file in &files {
        let content = read_source_text(file)?;
        let file_name = file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("module")
            .to_string();
        let canonical_file = canonical_or_original(file);
        let relative_file = canonical_file
            .strip_prefix(&canonical_root)
            .unwrap_or(&canonical_file)
            .to_string_lossy()
            .replace('\\', "/");

        components.push(Component {
            id: format!("ts:file:{relative_file}"),
            name: file_name,
            kind: ComponentKind::Module,
            language: "TypeScript".to_string(),
            file: canonical_file.to_string_lossy().to_string(),
        });

        for line in content.lines() {
            let normalized = line.trim();

            if let Some(name) = parse_named(normalized, "class") {
                components.push(Component {
                    id: format!("ts:class:{name}:{relative_file}"),
                    name,
                    kind: ComponentKind::Class,
                    language: "TypeScript".to_string(),
                    file: canonical_file.to_string_lossy().to_string(),
                });
            }

            if let Some(name) = parse_named(normalized, "function") {
                components.push(Component {
                    id: format!("ts:function:{name}:{relative_file}"),
                    name,
                    kind: ComponentKind::Function,
                    language: "TypeScript".to_string(),
                    file: canonical_file.to_string_lossy().to_string(),
                });
            }
        }
    }

    let by_file: BTreeMap<String, String> = components
        .iter()
        .filter(|component| component.kind == ComponentKind::Module)
        .map(|component| (component.file.clone(), component.id.clone()))
        .collect();

    let mut dependencies = Vec::new();

    for component in components
        .iter()
        .filter(|component| component.kind != ComponentKind::Module)
    {
        if let Some(module_id) = by_file.get(&component.file) {
            dependencies.push(Dependency {
                source_id: component.id.clone(),
                target_id: module_id.clone(),
                kind: DependencyKind::Requires,
            });
        }
    }

    for file in &files {
        let canonical_file = canonical_or_original(file);
        let source_key = canonical_file.to_string_lossy().to_string();
        let Some(source_id) = by_file.get(&source_key) else {
            continue;
        };
        let content = read_source_text(file).unwrap_or_default();

        for line in content.lines() {
            let normalized = line.trim_start();
            if !normalized.starts_with("import ") && !normalized.starts_with("export ") {
                continue;
            }

            let Some(specifier) = extract_module_specifier(line) else {
                continue;
            };

            let target_file = if specifier.starts_with('.') {
                resolve_from_base(file.parent().unwrap_or(root), &specifier)
            } else {
                config
                    .as_ref()
                    .and_then(|value| resolve_alias(value, &specifier))
            };

            let Some(target_file) = target_file else {
                continue;
            };
            let target_key = canonical_or_original(&target_file)
                .to_string_lossy()
                .to_string();

            if let Some(target_id) = by_file.get(&target_key) {
                if target_id != source_id {
                    dependencies.push(Dependency {
                        source_id: source_id.clone(),
                        target_id: target_id.clone(),
                        kind: DependencyKind::Imports,
                    });
                }
            }
        }
    }

    dependencies.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then(left.target_id.cmp(&right.target_id))
            .then(left.kind.as_str().cmp(right.kind.as_str()))
    });
    dependencies.dedup_by(|left, right| {
        left.source_id == right.source_id
            && left.target_id == right.target_id
            && left.kind == right.kind
    });

    Ok((components, dependencies))
}

pub fn discover_http_calls(root: &Path, components: &[Component]) -> io::Result<Vec<HttpCall>> {
    let mut by_file = BTreeMap::new();
    // Framework adapters may replace a module with a more specific component kind. Index every
    // source file, while still preferring the module node when one exists.
    for component in components {
        let file = canonical_or_original(Path::new(&component.file))
            .to_string_lossy()
            .to_string();
        if component.kind == ComponentKind::Module || !by_file.contains_key(&file) {
            by_file.insert(file, component.id.clone());
        }
    }
    let mut calls = Vec::new();

    for file in collect_http_source_files(root)? {
        let canonical = canonical_or_original(&file).to_string_lossy().to_string();
        let Some(component_id) = by_file.get(&canonical) else {
            continue;
        };
        let content = read_source_text(&file)?;
        let content = mask_javascript_comments(&content);
        let http_clients = http_client_identifiers(&content);

        for (needle, method) in [
            ("axios.get(", "GET"),
            ("axios.post(", "POST"),
            ("axios.put(", "PUT"),
            ("axios.patch(", "PATCH"),
            ("axios.delete(", "DELETE"),
        ] {
            collect_calls(
                &content,
                needle,
                method,
                component_id,
                &canonical,
                &mut calls,
            );
        }

        for identifier in http_clients {
            for method in ["get", "post", "put", "patch", "delete"] {
                collect_calls(
                    &content,
                    &format!("{identifier}.{method}("),
                    &method.to_ascii_uppercase(),
                    component_id,
                    &canonical,
                    &mut calls,
                );
            }
        }

        let mut offset = 0;
        while let Some(index) = content[offset..].find("fetch(") {
            let start = offset + index + 6;
            if let Some((expression, consumed)) = first_argument(&content[start..]) {
                if let Some(path) = pathname_from_expression(&expression) {
                    let tail_end = (start + consumed + 320).min(content.len());
                    let tail = &content[start + consumed..tail_end];
                    let method = fetch_method(tail).unwrap_or("GET");
                    calls.push(HttpCall {
                        method: method.to_string(),
                        path,
                        origin: origin_from_expression(&expression),
                        component_id: component_id.clone(),
                        file: canonical.clone(),
                        evidence: format!("fetch({expression})"),
                    });
                }
                offset = start + consumed;
            } else {
                offset = start;
            }
        }
    }

    calls.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.method.cmp(&right.method))
            .then(left.path.cmp(&right.path))
    });
    calls.dedup_by(|left, right| {
        left.file == right.file && left.method == right.method && left.path == right.path
    });
    Ok(calls)
}

fn mask_javascript_comments(content: &str) -> String {
    let chars = content.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(content.len());
    let mut index = 0usize;
    let mut quote = None;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if let Some(delimiter) = quote {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(current, '\'' | '"' | '`') {
            quote = Some(current);
            output.push(current);
            index += 1;
            continue;
        }
        if current == '/' && next == Some('/') {
            output.push(' ');
            output.push(' ');
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                output.push(' ');
                index += 1;
            }
            continue;
        }
        if current == '/' && next == Some('*') {
            output.push(' ');
            output.push(' ');
            index += 2;
            while index < chars.len() {
                if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    output.push(' ');
                    output.push(' ');
                    index += 2;
                    break;
                }
                output.push(if chars[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }
        output.push(current);
        index += 1;
    }
    output
}

fn collect_calls(
    content: &str,
    needle: &str,
    method: &str,
    component_id: &str,
    file: &str,
    calls: &mut Vec<HttpCall>,
) {
    let mut offset = 0;
    while let Some(index) = content[offset..].find(needle) {
        let start = offset + index + needle.len();
        if let Some((expression, consumed)) = first_argument(&content[start..]) {
            if let Some(path) = pathname_from_expression(&expression) {
                calls.push(HttpCall {
                    method: method.to_string(),
                    path,
                    origin: origin_from_expression(&expression),
                    component_id: component_id.to_string(),
                    file: file.to_string(),
                    evidence: format!("{needle}{expression})"),
                });
            }
            offset = start + consumed;
        } else {
            offset = start;
        }
    }
}

fn first_argument(value: &str) -> Option<(String, usize)> {
    let mut quote = None;
    let mut escaped = false;
    let mut nested = 0usize;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        if let Some(current) = quote {
            if character == current {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
        } else if matches!(character, '(' | '[' | '{') {
            nested += 1;
        } else if matches!(character, ')' | ']' | '}') && nested > 0 {
            nested -= 1;
        } else if nested == 0 && matches!(character, ',' | ')') {
            let expression = value[..index].trim();
            return (!expression.is_empty()).then(|| (expression.to_string(), index + 1));
        }
    }
    None
}

fn pathname_from_expression(expression: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut quote = None;
    let mut start = 0;
    let characters: Vec<(usize, char)> = expression.char_indices().collect();
    for (position, (index, character)) in characters.iter().enumerate() {
        if quote.is_none() && matches!(character, '\'' | '"' | '`') {
            quote = Some(*character);
            start = index + character.len_utf8();
        } else if quote == Some(*character) {
            parts.push(expression[start..*index].to_string());
            quote = None;
        } else if quote == Some('`')
            && *character == '$'
            && characters.get(position + 1).map(|(_, value)| *value) == Some('{')
        {
            parts.push("/{id}".trim_start_matches('/').to_string());
        }
    }
    let joined = parts.join("");
    if let Some((_, path)) = absolute_url_parts(&joined) {
        return Some(path);
    }
    let path_start = joined.find('/').or_else(|| expression.find('/'))?;
    let candidate = if joined.contains('/') {
        &joined[path_start..]
    } else {
        &expression[path_start..]
    };
    let cleaned = candidate
        .trim_matches(|character: char| matches!(character, '\'' | '"' | '`' | ' ' | ')' | ';'));
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn origin_from_expression(expression: &str) -> Option<String> {
    if let Some(start) = expression
        .find("https://")
        .or_else(|| expression.find("http://"))
    {
        return absolute_url_parts(&expression[start..]).map(|(origin, _)| origin);
    }
    let trimmed = expression.trim();
    let literal_relative = ['\'', '"', '`'].iter().any(|quote| {
        trimmed
            .strip_prefix(*quote)
            .is_some_and(|value| value.trim_start().starts_with('/'))
    });
    if literal_relative {
        None
    } else {
        Some("dynamic://unresolved".to_string())
    }
}

fn absolute_url_parts(value: &str) -> Option<(String, String)> {
    let start = value.find("https://").or_else(|| value.find("http://"))?;
    let value = &value[start..];
    let scheme_end = value.find("://")? + 3;
    let authority_end = value[scheme_end..]
        .find('/')
        .map(|index| scheme_end + index)
        .unwrap_or_else(|| {
            value
                .find(['\'', '"', '`', ' ', ')', ';'])
                .unwrap_or(value.len())
        });
    let origin = value[..authority_end]
        .trim_matches(|character: char| matches!(character, '\'' | '"' | '`' | ' '))
        .to_string();
    let path = if authority_end < value.len() && value.as_bytes()[authority_end] == b'/' {
        value[authority_end..]
            .trim_matches(|character: char| matches!(character, '\'' | '"' | '`' | ' ' | ')' | ';'))
            .to_string()
    } else {
        "/".to_string()
    };
    (!origin.is_empty()).then_some((origin, path))
}

fn http_client_identifiers(content: &str) -> Vec<String> {
    if !content.contains("HttpClient") {
        return Vec::new();
    }
    let mut identifiers = Vec::new();
    for segment in content.split([',', '(', ')']) {
        if let Some(index) = segment.find(": HttpClient") {
            let name = segment[..index]
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches("private")
                .trim();
            if !name.is_empty() {
                identifiers.push(name.to_string());
            }
        }
    }
    for line in content
        .lines()
        .filter(|line| line.contains("inject(HttpClient)"))
    {
        if let Some(left) = line.split('=').next() {
            let name = left.split_whitespace().last().unwrap_or("").trim();
            if !name.is_empty() {
                identifiers.push(name.to_string());
            }
        }
    }
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

fn fetch_method(tail: &str) -> Option<&'static str> {
    let upper = tail.to_ascii_uppercase();
    ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .into_iter()
        .find(|method| {
            upper.contains(&format!("METHOD: '{method}'"))
                || upper.contains(&format!("METHOD: \"{method}\""))
        })
}

fn parse_named(line: &str, keyword: &str) -> Option<String> {
    let patterns = [
        format!("export {keyword} "),
        format!("export default {keyword} "),
        format!("{keyword} "),
    ];

    for pattern in patterns {
        if let Some(index) = line.find(&pattern) {
            let tail = &line[index + pattern.len()..];
            let name = tail
                .split(|character: char| {
                    !character.is_alphanumeric() && character != '_' && character != '$'
                })
                .next()
                .unwrap_or("")
                .to_string();

            if !name.is_empty() {
                return Some(name);
            }
        }
    }

    None
}

fn extract_module_specifier(line: &str) -> Option<String> {
    let from_index = line.rfind(" from ");
    let search = from_index.map(|index| &line[index + 6..]).unwrap_or(line);

    for quote in ['"', '\''] {
        if let Some(start) = search.find(quote) {
            let tail = &search[start + 1..];
            if let Some(end) = tail.find(quote) {
                let value = tail[..end].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

fn load_typescript_config(root: &Path) -> Option<TypeScriptConfig> {
    let config_path = [root.join("tsconfig.json"), root.join("jsconfig.json")]
        .into_iter()
        .find(|path| path.exists())?;
    let content = read_source_text(config_path).ok()?;
    let sanitized = sanitize_json_like(&content);
    let value: Value = serde_json::from_str(&sanitized).ok()?;
    let compiler_options = value.get("compilerOptions")?;
    let base_url = compiler_options
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or(".");
    let base_dir = root.join(base_url);
    let mut aliases = Vec::new();

    if let Some(paths) = compiler_options.get("paths").and_then(Value::as_object) {
        for (pattern, target_value) in paths {
            let targets = target_value
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !targets.is_empty() {
                aliases.push(AliasRule {
                    pattern: pattern.clone(),
                    targets,
                });
            }
        }
    }

    Some(TypeScriptConfig { base_dir, aliases })
}

fn resolve_alias(config: &TypeScriptConfig, specifier: &str) -> Option<PathBuf> {
    for alias in &config.aliases {
        let Some(captured) = match_pattern(&alias.pattern, specifier) else {
            continue;
        };

        for target in &alias.targets {
            let mapped = if target.contains('*') {
                target.replace('*', &captured)
            } else {
                target.clone()
            };

            if let Some(path) = resolve_from_base(&config.base_dir, &mapped) {
                return Some(path);
            }
        }
    }

    None
}

fn match_pattern(pattern: &str, specifier: &str) -> Option<String> {
    if let Some(star) = pattern.find('*') {
        let prefix = &pattern[..star];
        let suffix = &pattern[star + 1..];

        if specifier.starts_with(prefix)
            && specifier.ends_with(suffix)
            && specifier.len() >= prefix.len() + suffix.len()
        {
            let end = specifier.len() - suffix.len();
            return Some(specifier[prefix.len()..end].to_string());
        }

        return None;
    }

    if pattern == specifier {
        Some(String::new())
    } else {
        None
    }
}

fn resolve_from_base(base: &Path, specifier: &str) -> Option<PathBuf> {
    let raw = base.join(specifier);
    let candidates = [
        raw.clone(),
        raw.with_extension("ts"),
        raw.with_extension("tsx"),
        raw.with_extension("js"),
        raw.with_extension("jsx"),
        raw.join("index.ts"),
        raw.join("index.tsx"),
        raw.join("index.js"),
        raw.join("index.jsx"),
    ];

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|candidate| canonical_or_original(&candidate))
}

fn sanitize_json_like(content: &str) -> String {
    let chars: Vec<char> = content.chars().collect();
    let mut output = String::with_capacity(chars.len());
    let mut index = 0;
    let mut string_literal = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if line_comment {
            if current == '\n' {
                line_comment = false;
                output.push('\n');
            }
            index += 1;
            continue;
        }

        if block_comment {
            if current == '*' && next == Some('/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if string_literal {
            output.push(current);
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

        if current == '"' {
            string_literal = true;
            output.push(current);
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            line_comment = true;
            index += 2;
            continue;
        }

        if current == '/' && next == Some('*') {
            block_comment = true;
            index += 2;
            continue;
        }

        output.push(current);
        index += 1;
    }

    let chars: Vec<char> = output.chars().collect();
    let mut cleaned = String::with_capacity(chars.len());
    let mut string_literal = false;
    let mut escaped = false;
    let mut index = 0;

    while index < chars.len() {
        let current = chars[index];

        if string_literal {
            cleaned.push(current);
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

        if current == '"' {
            string_literal = true;
            cleaned.push(current);
            index += 1;
            continue;
        }

        if current == ',' {
            let next_significant = chars[index + 1..]
                .iter()
                .copied()
                .find(|character| !character.is_whitespace());
            if matches!(next_significant, Some('}') | Some(']')) {
                index += 1;
                continue;
            }
        }

        cleaned.push(current);
        index += 1;
    }

    cleaned
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn collect_typescript_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_http_source_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    visit_with_extensions(
        root,
        root,
        &mut files,
        &[
            "ts", "tsx", "js", "jsx", "mjs", "cjs", "vue", "svelte", "astro",
        ],
    )?;
    files.sort();
    Ok(files)
}

fn visit(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    visit_with_extensions(root, path, files, &["ts", "tsx"])
}

fn visit_with_extensions(
    root: &Path,
    path: &Path,
    files: &mut Vec<PathBuf>,
    extensions: &[&str],
) -> io::Result<()> {
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
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
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
            visit_with_extensions(root, &entry.path(), files, extensions)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_alias_wildcard() {
        assert_eq!(
            match_pattern("@/*", "@/services/auth"),
            Some("services/auth".to_string())
        );
        assert_eq!(match_pattern("@core/*", "@shared/model"), None);
    }

    #[test]
    fn removes_json_comments_and_trailing_commas() {
        let value = sanitize_json_like("{\n\"compilerOptions\": {\"baseUrl\": \".\",},\n}\n");
        let parsed: Value = serde_json::from_str(&value).unwrap();
        assert_eq!(parsed["compilerOptions"]["baseUrl"].as_str(), Some("."));
    }

    #[test]
    fn masks_http_calls_inside_comments() {
        let masked = mask_javascript_comments(
            "// fetch('/ignored')\n/* axios.get('/also-ignored') */\nfetch('/real')",
        );
        assert!(!masked.contains("/ignored"));
        assert!(!masked.contains("/also-ignored"));
        assert!(masked.contains("fetch('/real')"));
    }

    #[test]
    fn extracts_static_path_from_base_url_expression() {
        assert_eq!(
            pathname_from_expression("environment.apiUrl + \"/api/users\""),
            Some("/api/users".to_string())
        );
        assert_eq!(
            pathname_from_expression("`/api/users/${id}`"),
            Some("/api/users/${id}".to_string())
        );
        assert_eq!(
            pathname_from_expression("\"https://api.example.com/v1/users\""),
            Some("/v1/users".to_string())
        );
        assert_eq!(
            origin_from_expression("\"https://api.example.com/v1/users\""),
            Some("https://api.example.com".to_string())
        );
        assert_eq!(
            origin_from_expression("environment.apiUrl + \"/api/users\""),
            Some("dynamic://unresolved".to_string())
        );
        assert_eq!(origin_from_expression("\"/api/users\""), None);
    }

    #[test]
    fn discovers_calls_in_javascript_and_preserves_url_certainty() {
        let root = std::env::temp_dir().join(format!("reposlice-http-js-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("client.js");
        fs::write(
            &file,
            "fetch('/api/users');\naxios.get('https://api.example.com/v1/items');\nfetch(environment.apiUrl + '/api/orders');",
        )
        .unwrap();
        let canonical = fs::canonicalize(&file)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let calls = discover_http_calls(
            &root,
            &[Component {
                id: "client".into(),
                name: "client".into(),
                kind: ComponentKind::Module,
                language: "JavaScript".into(),
                file: canonical,
            }],
        )
        .unwrap();
        assert_eq!(calls.len(), 3);
        assert!(calls
            .iter()
            .any(|call| call.path == "/api/users" && call.origin.is_none()));
        assert!(calls.iter().any(|call| {
            call.path == "/v1/items" && call.origin.as_deref() == Some("https://api.example.com")
        }));
        assert!(calls.iter().any(|call| {
            call.path == "/api/orders" && call.origin.as_deref() == Some("dynamic://unresolved")
        }));
        fs::remove_dir_all(root).unwrap();
    }
}
