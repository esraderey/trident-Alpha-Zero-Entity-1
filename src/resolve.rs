use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Information about a discovered module.
#[derive(Clone, Debug)]
pub struct ModuleInfo {
    /// Dotted module name (e.g. "crypto.sponge").
    pub name: String,
    /// Filesystem path to the .tri file.
    pub file_path: PathBuf,
    /// Source code.
    pub source: String,
    /// Modules this module depends on (from `use` statements).
    pub dependencies: Vec<String>,
}

/// Resolve all modules reachable from an entry point.
/// Returns modules in topological order (dependencies first).
pub fn resolve_modules(entry_path: &Path) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
    let mut resolver = ModuleResolver::new(entry_path)?;
    resolver.discover_all()?;
    resolver.topological_sort()
}

struct ModuleResolver {
    /// Root directory of the project.
    root_dir: PathBuf,
    /// All discovered modules by name.
    modules: HashMap<String, ModuleInfo>,
    /// Queue of modules to process.
    queue: Vec<String>,
    /// Diagnostics.
    diagnostics: Vec<Diagnostic>,
}

impl ModuleResolver {
    fn new(entry_path: &Path) -> Result<Self, Vec<Diagnostic>> {
        let root_dir = entry_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();

        let source = std::fs::read_to_string(entry_path).map_err(|e| {
            vec![Diagnostic::error(
                format!("cannot read '{}': {}", entry_path.display(), e),
                Span::dummy(),
            )]
        })?;

        // Quick-parse the entry file to get its name and dependencies
        let (name, deps) = scan_module_header(&source);
        let entry_name = name.unwrap_or_else(|| "main".to_string());

        let info = ModuleInfo {
            name: entry_name.clone(),
            file_path: entry_path.to_path_buf(),
            source,
            dependencies: deps.clone(),
        };

        let mut modules = HashMap::new();
        modules.insert(entry_name.clone(), info);

        Ok(Self {
            root_dir,
            modules,
            queue: deps,
            diagnostics: Vec::new(),
        })
    }

    fn discover_all(&mut self) -> Result<(), Vec<Diagnostic>> {
        while let Some(module_name) = self.queue.pop() {
            if self.modules.contains_key(&module_name) {
                continue;
            }

            // Resolve module name to file path
            let file_path = self.resolve_path(&module_name);
            let source = match std::fs::read_to_string(&file_path) {
                Ok(s) => s,
                Err(e) => {
                    self.diagnostics.push(Diagnostic::error(
                        format!(
                            "cannot find module '{}' (looked at '{}'): {}",
                            module_name,
                            file_path.display(),
                            e
                        ),
                        Span::dummy(),
                    ));
                    continue;
                }
            };

            let (_name, deps) = scan_module_header(&source);

            // Queue newly discovered dependencies
            for dep in &deps {
                if !self.modules.contains_key(dep) {
                    self.queue.push(dep.clone());
                }
            }

            self.modules.insert(
                module_name.clone(),
                ModuleInfo {
                    name: module_name,
                    file_path,
                    source,
                    dependencies: deps,
                },
            );
        }

        if self.diagnostics.is_empty() {
            Ok(())
        } else {
            Err(self.diagnostics.clone())
        }
    }

    /// Resolve a dotted module name to a file path.
    /// "crypto.sponge" → root_dir/crypto/sponge.tri
    /// "merkle" → root_dir/merkle.tri
    fn resolve_path(&self, module_name: &str) -> PathBuf {
        let parts: Vec<&str> = module_name.split('.').collect();
        let mut path = self.root_dir.clone();
        for part in &parts {
            path = path.join(part);
        }
        path.with_extension("tri")
    }

    /// Topological sort of the module DAG. Returns Err if circular.
    fn topological_sort(&self) -> Result<Vec<ModuleInfo>, Vec<Diagnostic>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut in_progress: HashSet<String> = HashSet::new();
        let mut order: Vec<String> = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        for name in self.modules.keys() {
            if !visited.contains(name) {
                self.dfs(
                    name,
                    &mut visited,
                    &mut in_progress,
                    &mut order,
                    &mut diagnostics,
                );
            }
        }

        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }

        let result: Vec<ModuleInfo> = order
            .into_iter()
            .filter_map(|name| self.modules.get(&name).cloned())
            .collect();

        Ok(result)
    }

    fn dfs(
        &self,
        name: &str,
        visited: &mut HashSet<String>,
        in_progress: &mut HashSet<String>,
        order: &mut Vec<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if visited.contains(name) {
            return;
        }
        if in_progress.contains(name) {
            diagnostics.push(Diagnostic::error(
                format!("circular dependency detected involving module '{}'", name),
                Span::dummy(),
            ));
            return;
        }

        in_progress.insert(name.to_string());

        if let Some(info) = self.modules.get(name) {
            for dep in &info.dependencies {
                self.dfs(dep, visited, in_progress, order, diagnostics);
            }
        }

        in_progress.remove(name);
        visited.insert(name.to_string());
        order.push(name.to_string());
    }
}

/// Quick scan of a source file to extract module name and `use` dependencies.
/// Does not fully parse — just looks for `program X` / `module X` and `use Y` lines.
fn scan_module_header(source: &str) -> (Option<String>, Vec<String>) {
    let mut name = None;
    let mut deps = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("program ") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("module ") {
            name = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("use ") {
            let dep = rest.trim().to_string();
            // Don't add std.* as file dependencies — they're built-in
            if !dep.starts_with("std.") && !dep.starts_with("std") {
                deps.push(dep);
            }
        } else {
            // Once we hit a non-header line, stop scanning for use statements
            // (use must come before items per the grammar)
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("pub ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("struct ")
            {
                break;
            }
        }
    }

    (name, deps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_module_header_program() {
        let (name, deps) = scan_module_header("program my_app\n\nuse merkle\nuse crypto.sponge\n\nfn main() {}");
        assert_eq!(name, Some("my_app".to_string()));
        assert_eq!(deps, vec!["merkle", "crypto.sponge"]);
    }

    #[test]
    fn test_scan_module_header_module() {
        let (name, deps) = scan_module_header("module merkle\n\nuse std.convert\n\npub fn verify() {}");
        assert_eq!(name, Some("merkle".to_string()));
        assert!(deps.is_empty()); // std.* is filtered out
    }

    #[test]
    fn test_scan_module_header_no_deps() {
        let (name, deps) = scan_module_header("program simple\n\nfn main() {}");
        assert_eq!(name, Some("simple".to_string()));
        assert!(deps.is_empty());
    }
}
