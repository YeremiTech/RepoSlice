use reposlice_core::RuntimeRequirement;
use std::collections::BTreeSet;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTool {
    pub name: String,
    pub executable: String,
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeCapabilities {
    pub docker: bool,
    pub java: bool,
    pub node: bool,
    pub python: bool,
    pub php: bool,
    pub composer: bool,
    pub tools: Vec<RuntimeTool>,
}

pub fn detect_runtime() -> RuntimeCapabilities {
    detect_tools(&[
        ("Docker", "docker"),
        ("Java", "java"),
        ("Node.js", "node"),
        ("Python", "python"),
        ("PHP", "php"),
        ("Composer", "composer"),
        (".NET", "dotnet"),
        ("Go", "go"),
        ("Rust", "rustc"),
        ("Cargo", "cargo"),
        ("Ruby", "ruby"),
        ("Elixir", "elixir"),
        ("Mix", "mix"),
    ])
}

pub fn detect_project_runtime(requirements: &[RuntimeRequirement]) -> RuntimeCapabilities {
    let unique: BTreeSet<(String, String)> = requirements
        .iter()
        .map(|item| (item.name.clone(), item.executable.clone()))
        .collect();
    let requested: Vec<(&str, &str)> = unique
        .iter()
        .map(|(name, executable)| (name.as_str(), executable.as_str()))
        .collect();
    detect_tools(&requested)
}

fn detect_tools(requested: &[(&str, &str)]) -> RuntimeCapabilities {
    let tools: Vec<RuntimeTool> = requested
        .iter()
        .map(|(name, executable)| RuntimeTool {
            name: (*name).into(),
            executable: (*executable).into(),
            available: available(executable, &["--version"]),
            version: version(executable),
        })
        .collect();
    let has = |executable: &str| {
        tools
            .iter()
            .any(|tool| tool.executable == executable && tool.available)
    };
    RuntimeCapabilities {
        docker: has("docker"),
        java: has("java"),
        node: has("node"),
        python: has("python"),
        php: has("php"),
        composer: has("composer"),
        tools,
    }
}

fn available(command: &str, arguments: &[&str]) -> bool {
    Command::new(command)
        .args(arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn version(command: &str) -> Option<String> {
    let output = Command::new(command).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    (!value.is_empty()).then(|| {
        value
            .lines()
            .next()
            .unwrap_or(value)
            .chars()
            .take(160)
            .collect()
    })
}
