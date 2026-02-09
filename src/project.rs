use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Minimal project configuration from trident.toml.
#[derive(Clone, Debug)]
pub struct Project {
    pub name: String,
    pub version: String,
    pub entry: PathBuf,
    pub root_dir: PathBuf,
}

impl Project {
    /// Load project from a trident.toml file.
    pub fn load(toml_path: &Path) -> Result<Project, Diagnostic> {
        let content = std::fs::read_to_string(toml_path).map_err(|e| {
            Diagnostic::error(
                format!("cannot read '{}': {}", toml_path.display(), e),
                Span::dummy(),
            )
        })?;

        let root_dir = toml_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();

        // Minimal TOML parsing: just extract [project] fields
        let mut name = String::new();
        let mut version = String::new();
        let mut entry = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() || trimmed.starts_with('[') {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim().trim_matches('"');
                let value = value.trim().trim_matches('"');
                match key {
                    "name" => name = value.to_string(),
                    "version" => version = value.to_string(),
                    "entry" => entry = value.to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err(Diagnostic::error(
                "missing 'name' in trident.toml".to_string(),
                Span::dummy(),
            ));
        }

        if entry.is_empty() {
            entry = "main.tri".to_string();
        }

        Ok(Project {
            name,
            version,
            entry: root_dir.join(&entry),
            root_dir,
        })
    }

    /// Try to find a trident.toml in the given directory or its ancestors.
    pub fn find(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("trident.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_project() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("trident.toml");
        fs::write(
            &toml_path,
            r#"[project]
name = "test_project"
version = "0.1.0"
entry = "main.tri"
"#,
        )
        .unwrap();

        let project = Project::load(&toml_path).unwrap();
        assert_eq!(project.name, "test_project");
        assert_eq!(project.version, "0.1.0");
        assert!(project.entry.ends_with("main.tri"));
    }
}
