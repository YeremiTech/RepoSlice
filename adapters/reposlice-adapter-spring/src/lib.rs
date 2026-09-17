use reposlice_core::{
    read_source_text, AnalysisContribution, Component, ComponentKind, Entrypoint, EntrypointKind,
    Evidence, EvidenceKind, FrameworkAdapter, FrameworkDetection, Technology,
    DEFAULT_MAX_SOURCE_SIZE,
};
use reposlice_parser_java::collect_java_files;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct SpringAdapter;

impl FrameworkAdapter for SpringAdapter {
    fn id(&self) -> &'static str {
        "spring-boot"
    }
    fn detect(&self, root: &Path) -> FrameworkDetection {
        let manifest_path = [
            root.join("pom.xml"),
            root.join("build.gradle"),
            root.join("build.gradle.kts"),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| root.join("pom.xml"));
        let manifest = read_source_text(&manifest_path).unwrap_or_default();
        let files = collect_java_files(root).unwrap_or_default();
        detect_spring(&manifest_path, &manifest, &files)
    }
    fn analyze(&self, root: &Path) -> io::Result<AnalysisContribution> {
        let (mut components, dependencies) = reposlice_parser_java::parse_java(root)?;
        enrich_components(&mut components);
        let entrypoints = discover_entrypoints(root, &components)?;
        Ok(AnalysisContribution {
            technologies: vec![Technology {
                category: "framework".into(),
                name: "Spring Boot".into(),
                confidence: self.detect(root).confidence,
                classification: "Backend Framework".into(),
                ..Technology::default()
            }],
            components,
            entrypoints,
            dependencies,
            ..Default::default()
        })
    }
}

pub fn enrich_components(components: &mut [Component]) {
    let mut cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for component in components {
        let content = cache
            .entry(component.file.clone())
            .or_insert_with(|| read_source_text(&component.file).unwrap_or_default());

        if content.contains("@RestController") || content.contains("@Controller") {
            component.kind = ComponentKind::Controller;
        } else if content.contains("@Service") {
            component.kind = ComponentKind::Service;
        } else if content.contains("@Repository") {
            component.kind = ComponentKind::Repository;
        } else if content.contains("@Entity") {
            component.kind = ComponentKind::Entity;
        }
    }
}

pub fn discover_entrypoints(root: &Path, components: &[Component]) -> io::Result<Vec<Entrypoint>> {
    let mut entrypoints = Vec::new();

    for file in collect_java_files(root)? {
        let content = read_source_text(&file)?;
        let file_string = file.to_string_lossy().to_string();
        let component_id = components
            .iter()
            .find(|component| component.file == file_string)
            .map(|component| component.id.clone())
            .unwrap_or_else(|| format!("java:file:{file_string}"));
        let class_line = first_type_line(&content).unwrap_or(0);
        let base_paths = class_request_paths(&content, class_line);
        let annotations = annotation_expressions(&content);

        for (line_index, expression) in annotations {
            if line_index < class_line {
                continue;
            }

            let mappings = mapping(&expression);
            for (method, method_paths) in mappings {
                for base_path in &base_paths {
                    for method_path in &method_paths {
                        let path = merge_paths(base_path, method_path);
                        // Include the owning type so identical routes in distinct
                        // controllers cannot collapse into one entrypoint.
                        let id = format!("http:{component_id}:{method}:{path}");

                        entrypoints.push(Entrypoint {
                            id: id.clone(),
                            name: format!("{method} {path}"),
                            kind: EntrypointKind::HttpEndpoint,
                            component_id: component_id.clone(),
                            file: file_string.clone(),
                        });
                    }
                }
            }
        }
    }

    entrypoints.sort_by(|left, right| left.name.cmp(&right.name));
    entrypoints.dedup_by(|left, right| left.id == right.id);

    Ok(entrypoints)
}

fn first_type_line(content: &str) -> Option<usize> {
    content.lines().enumerate().find_map(|(index, line)| {
        let normalized = line.replace(['{', '(', ')', '<', '>'], " ");
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        let is_type = ["class", "interface", "record", "enum"]
            .iter()
            .any(|keyword| tokens.iter().any(|token| token == keyword));
        is_type.then_some(index)
    })
}

fn class_request_paths(content: &str, class_line: usize) -> Vec<String> {
    let mut paths = Vec::new();

    for (line_index, expression) in annotation_expressions(content) {
        if line_index > class_line {
            break;
        }

        if expression.starts_with("@RequestMapping") {
            paths = annotation_paths(&expression);
        }
    }

    if paths.is_empty() {
        vec![String::new()]
    } else {
        paths
    }
}

fn mapping(expression: &str) -> Vec<(String, Vec<String>)> {
    let direct = [
        ("@GetMapping", "GET"),
        ("@PostMapping", "POST"),
        ("@PutMapping", "PUT"),
        ("@DeleteMapping", "DELETE"),
        ("@PatchMapping", "PATCH"),
    ];

    for (annotation, method) in direct {
        if expression.starts_with(annotation) {
            return vec![(method.to_string(), annotation_paths(expression))];
        }
    }

    if expression.starts_with("@RequestMapping") && expression.contains("RequestMethod.") {
        let paths = annotation_paths(expression);
        return ["GET", "POST", "PUT", "DELETE", "PATCH"]
            .iter()
            .filter(|method| expression.contains(&format!("RequestMethod.{method}")))
            .map(|method| ((*method).to_string(), paths.clone()))
            .collect();
    }

    Vec::new()
}

fn annotation_paths(expression: &str) -> Vec<String> {
    let Some(open) = expression.find('(') else {
        return vec![String::new()];
    };
    let arguments = expression[open + 1..].trim_end_matches(')').trim();

    for key in ["path", "value"] {
        if let Some(value_expression) = attribute_expression(arguments, key) {
            let values = quoted_strings(value_expression);
            return if values.is_empty() {
                vec![String::new()]
            } else {
                values
            };
        }
    }

    let first_argument = first_argument_expression(arguments);
    if !first_argument.contains('=') {
        let values = quoted_strings(first_argument);
        if !values.is_empty() {
            return values;
        }
    }

    vec![String::new()]
}

fn first_argument_expression(arguments: &str) -> &str {
    let mut brace_depth = 0;
    let mut bracket_depth = 0;
    let mut string_literal = false;
    let mut escaped = false;

    for (index, character) in arguments.char_indices() {
        if string_literal {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                string_literal = false;
            }
            continue;
        }

        match character {
            '"' => string_literal = true,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && bracket_depth == 0 => return arguments[..index].trim(),
            _ => {}
        }
    }

    arguments.trim()
}

fn attribute_expression<'a>(arguments: &'a str, key: &str) -> Option<&'a str> {
    for (key_index, _) in arguments.match_indices(key) {
        let before = arguments[..key_index].chars().next_back();
        let after_key = key_index + key.len();
        let after = arguments[after_key..].chars().next();

        if before.map(is_identifier_character).unwrap_or(false)
            || after.map(is_identifier_character).unwrap_or(false)
        {
            continue;
        }

        let remainder = &arguments[after_key..];
        let trimmed = remainder.trim_start();
        if !trimmed.starts_with('=') {
            continue;
        }

        let whitespace = remainder.len() - trimmed.len();
        let start = after_key + whitespace + 1;
        let tail = &arguments[start..];
        let mut brace_depth = 0;
        let mut bracket_depth = 0;
        let mut string_literal = false;
        let mut escaped = false;

        for (index, character) in tail.char_indices() {
            if string_literal {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    string_literal = false;
                }
                continue;
            }

            match character {
                '"' => string_literal = true,
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                '[' => bracket_depth += 1,
                ']' => bracket_depth -= 1,
                ',' if brace_depth == 0 && bracket_depth == 0 => return Some(tail[..index].trim()),
                _ => {}
            }
        }

        return Some(tail.trim());
    }

    None
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn quoted_strings(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut results = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '"' {
            index += 1;
            continue;
        }

        index += 1;
        let mut current = String::new();
        let mut escaped = false;

        while index < chars.len() {
            let character = chars[index];
            if escaped {
                current.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                break;
            } else {
                current.push(character);
            }
            index += 1;
        }

        results.push(current);
        index += 1;
    }

    results
}

fn annotation_expressions(content: &str) -> Vec<(usize, String)> {
    let mut expressions = Vec::new();
    let bytes = content.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let Some(relative) = content[offset..].find('@') else {
            break;
        };
        let start = offset + relative;
        let mut end = start + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'.'))
        {
            end += 1;
        }
        if end == start + 1 {
            offset = end;
            continue;
        }
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        if end < bytes.len() && bytes[end] == b'(' {
            let mut index = end;
            let mut balance = 0i32;
            let mut string_literal = false;
            let mut escaped = false;
            while index < bytes.len() {
                let character = bytes[index] as char;
                if string_literal {
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        string_literal = false;
                    }
                } else if character == '"' {
                    string_literal = true;
                } else if character == '(' {
                    balance += 1;
                } else if character == ')' {
                    balance -= 1;
                    if balance == 0 {
                        index += 1;
                        break;
                    }
                }
                index += 1;
            }
            end = index;
        }
        let line = content[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        expressions.push((line, content[start..end].trim().replace(['\r', '\n'], " ")));
        offset = end.max(start + 1);
    }
    expressions
}

fn merge_paths(base: &str, method: &str) -> String {
    let joined = format!("{}/{}", base.trim_matches('/'), method.trim_matches('/'));
    let normalized = format!("/{}", joined.trim_matches('/'));

    if normalized == "/" {
        normalized
    } else {
        normalized.replace("//", "/")
    }
}

pub fn detect_spring(
    manifest_path: &Path,
    manifest_content: &str,
    files: &[PathBuf],
) -> FrameworkDetection {
    let mut confidence = 0u8;
    let mut evidence = Vec::new();
    if manifest_content.contains("spring-boot")
        || manifest_content.contains("org.springframework.boot")
    {
        confidence += 60;
        evidence.push(Evidence {
            kind: EvidenceKind::Manifest,
            source: manifest_path.to_string_lossy().into_owned(),
            detail: "Build manifest references Spring Boot".into(),
            confidence: 100,
        });
    }
    let mut annotation_file = None;
    for file in files
        .iter()
        .filter(|file| file.extension().and_then(|value| value.to_str()) == Some("java"))
    {
        if fs::metadata(file)
            .map(|value| value.len() <= DEFAULT_MAX_SOURCE_SIZE)
            .unwrap_or(false)
        {
            let content = read_source_text(file).unwrap_or_default();
            if content.contains("@SpringBootApplication") || content.contains("@RestController") {
                annotation_file = Some(file.clone());
                break;
            }
        }
    }
    if let Some(file) = annotation_file {
        confidence = confidence.saturating_add(30);
        evidence.push(Evidence {
            kind: EvidenceKind::Source,
            source: file.to_string_lossy().into_owned(),
            detail: "Spring annotation found in Java source".into(),
            confidence: 95,
        });
    }
    FrameworkDetection {
        name: "Spring Boot".into(),
        confidence: confidence.min(100),
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_multiple_paths() {
        let paths = annotation_paths("@RequestMapping({\"/users\", \"/accounts\"})");
        assert_eq!(paths, vec!["/users", "/accounts"]);
    }

    #[test]
    fn maps_request_mapping_methods() {
        let mappings = mapping(
            "@RequestMapping(path = \"/users\", method = {RequestMethod.GET, RequestMethod.POST})",
        );
        assert_eq!(mappings.len(), 2);
    }

    #[test]
    fn ignores_non_path_string_attributes() {
        let paths = annotation_paths(
            "@RequestMapping(value = \"/path/users\", method = RequestMethod.GET, produces = \"application/json\")",
        );
        assert_eq!(paths, vec!["/path/users"]);
    }

    #[test]
    fn reads_positional_path_with_named_method() {
        let paths = annotation_paths("@RequestMapping(\"/users\", method = RequestMethod.GET)");
        assert_eq!(paths, vec!["/users"]);
    }

    #[test]
    fn finds_annotations_embedded_on_declaration_lines() {
        let expressions = annotation_expressions(
            "@RestController @RequestMapping(\"/api\")\nclass Api { @GetMapping(\"/health\") String health() {} }",
        );
        assert!(expressions
            .iter()
            .any(|(_, value)| value == "@RequestMapping(\"/api\")"));
        assert!(expressions
            .iter()
            .any(|(_, value)| value == "@GetMapping(\"/health\")"));
    }
}
