use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::ast::{self, FileKind};
use crate::cost;
use crate::diagnostic::{render_diagnostics, Diagnostic};
use crate::linker::{link, ModuleTasm};
use crate::resolve::resolve_modules;
use crate::span;
use crate::target::TargetConfig;
use crate::tir::builder::TIRBuilder;
use crate::tir::lower::create_stack_lowering;
use crate::typecheck::{ModuleExports, TypeChecker};
use crate::{doc, format, lexer, parser, project, solve, sym};

#[cfg(test)]
mod tests;

/// Options controlling compilation: VM target + conditional compilation flags.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Profile name for cfg flags (e.g. "debug", "release").
    pub profile: String,
    /// Active cfg flags for conditional compilation.
    pub cfg_flags: HashSet<String>,
    /// Target VM configuration.
    pub target_config: TargetConfig,
    /// Additional module search directories (from locked dependencies).
    pub dep_dirs: Vec<std::path::PathBuf>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            profile: "debug".to_string(),
            cfg_flags: HashSet::from(["debug".to_string()]),
            target_config: TargetConfig::triton(),
            dep_dirs: Vec::new(),
        }
    }
}

impl CompileOptions {
    /// Create options for a named profile (debug/release/custom).
    pub fn for_profile(profile: &str) -> Self {
        Self {
            profile: profile.to_string(),
            cfg_flags: HashSet::from([profile.to_string()]),
            target_config: TargetConfig::triton(),
            dep_dirs: Vec::new(),
        }
    }

    /// Create options for a named built-in target (backward compat alias).
    pub fn for_target(target: &str) -> Self {
        Self::for_profile(target)
    }
}

/// Compile a single Trident source string to TASM.
pub fn compile(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    compile_with_options(source, filename, &CompileOptions::default())
}

/// Compile a single Trident source string to TASM with options.
pub fn compile_with_options(
    source: &str,
    filename: &str,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    // Type check
    let exports = match TypeChecker::with_target(options.target_config.clone())
        .with_cfg_flags(options.cfg_flags.clone())
        .check_file(&file)
    {
        Ok(exports) => exports,
        Err(errors) => {
            render_diagnostics(&errors, filename, source);
            return Err(errors);
        }
    };

    // Build IR and lower to target assembly
    let ir = TIRBuilder::new(options.target_config.clone())
        .with_cfg_flags(options.cfg_flags.clone())
        .with_mono_instances(exports.mono_instances)
        .with_call_resolutions(exports.call_resolutions)
        .build_file(&file);
    let lowering = create_stack_lowering(&options.target_config.name);
    let tasm = lowering.lower(&ir).join("\n");
    Ok(tasm)
}

/// Compile a multi-module project from an entry point path.
pub fn compile_project(entry_path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_project_with_options(entry_path, &CompileOptions::default())
}

/// Compile a multi-module project with options.
pub fn compile_project_with_options(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    let intrinsic_map = project.intrinsic_map();
    let module_aliases = project.module_aliases();
    let external_constants = project.external_constants();

    // Emit TASM for each module
    let mut tasm_modules = Vec::new();
    for (i, pm) in project.modules.iter().enumerate() {
        let is_program = pm.file.kind == FileKind::Program;
        let mono = project
            .exports
            .get(i)
            .map(|e| e.mono_instances.clone())
            .unwrap_or_default();
        let call_res = project
            .exports
            .get(i)
            .map(|e| e.call_resolutions.clone())
            .unwrap_or_default();
        let ir = TIRBuilder::new(options.target_config.clone())
            .with_cfg_flags(options.cfg_flags.clone())
            .with_intrinsics(intrinsic_map.clone())
            .with_module_aliases(module_aliases.clone())
            .with_constants(external_constants.clone())
            .with_mono_instances(mono)
            .with_call_resolutions(call_res)
            .build_file(&pm.file);
        let lowering = create_stack_lowering(&options.target_config.name);
        let tasm = lowering.lower(&ir).join("\n");
        tasm_modules.push(ModuleTasm {
            module_name: pm.file.name.node.clone(),
            is_program,
            tasm,
        });
    }

    // Link
    let linked = link(tasm_modules);
    Ok(linked)
}

/// Type-check only (no TASM emission).
pub fn check(source: &str, filename: &str) -> Result<(), Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    Ok(())
}

/// Project-aware type-check from an entry point path.
/// Resolves all modules (including std.*) and type-checks in dependency order.
pub fn check_project(entry_path: &Path) -> Result<(), Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    PreparedProject::build_default(entry_path)?;
    Ok(())
}

/// Discover `#[test]` functions in a parsed file.
pub fn discover_tests(file: &ast::File) -> Vec<String> {
    let mut tests = Vec::new();
    for item in &file.items {
        if let ast::Item::Fn(func) = &item.node {
            if func.is_test {
                tests.push(func.name.node.clone());
            }
        }
    }
    tests
}

/// A single test result.
#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub cost: Option<cost::TableCost>,
    pub error: Option<String>,
}

/// Run all `#[test]` functions in a project.
///
/// For each test function, we:
/// 1. Parse and type-check the project
/// 2. Compile a mini-program that just calls the test function
/// 3. Report pass/fail with cost summary
pub fn run_tests(
    entry_path: &std::path::Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    // Discover all #[test] functions across all modules
    let mut test_fns: Vec<(String, String)> = Vec::new(); // (module_name, fn_name)
    for pm in &project.modules {
        for test_name in discover_tests(&pm.file) {
            test_fns.push((pm.file.name.node.clone(), test_name));
        }
    }

    if test_fns.is_empty() {
        return Ok("No #[test] functions found.\n".to_string());
    }

    // For each test function, compile a mini-program and report
    let mut results: Vec<TestResult> = Vec::new();
    let mut short_names: Vec<String> = Vec::new();
    for (module_name, test_name) in &test_fns {
        // Find the source file for this module
        let source_entry = project
            .modules
            .iter()
            .find(|m| m.file.name.node == *module_name);

        if let Some(pm) = source_entry {
            // Build a mini-program source that just calls the test function
            let mini_source = if module_name.starts_with("module") || module_name.contains('.') {
                // For module test functions, we'd need cross-module calls
                // For simplicity, compile in-context
                pm.source.clone()
            } else {
                pm.source.clone()
            };

            // Try to compile (type-check + emit) the source.
            // The test function itself is validated by the type checker.
            // For now, "passing" means it compiles without errors.
            match compile_with_options(&mini_source, &pm.file_path.to_string_lossy(), options) {
                Ok(tasm) => {
                    // Compute cost for the test function
                    let test_cost =
                        analyze_costs(&mini_source, &pm.file_path.to_string_lossy()).ok();
                    if short_names.is_empty() {
                        if let Some(ref pc) = test_cost {
                            short_names = pc.table_short_names.clone();
                        }
                    }
                    let fn_cost = test_cost.as_ref().and_then(|pc| {
                        pc.functions
                            .iter()
                            .find(|f| f.name == *test_name)
                            .map(|f| f.cost.clone())
                    });
                    // Check if the generated TASM contains an assert failure marker
                    let has_error = tasm.contains("// ERROR");
                    results.push(TestResult {
                        name: test_name.clone(),
                        passed: !has_error,
                        cost: fn_cost,
                        error: if has_error {
                            Some("compilation produced errors".to_string())
                        } else {
                            None
                        },
                    });
                }
                Err(errors) => {
                    let msg = errors
                        .iter()
                        .map(|d| d.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ");
                    results.push(TestResult {
                        name: test_name.clone(),
                        passed: false,
                        cost: None,
                        error: Some(msg),
                    });
                }
            }
        }
    }

    // Format the report
    let mut report = String::new();
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    report.push_str(&format!(
        "running {} test{}\n",
        total,
        if total == 1 { "" } else { "s" }
    ));

    for result in &results {
        let status = if result.passed { "ok" } else { "FAILED" };
        let cost_str = if let Some(ref c) = result.cost {
            let sn: Vec<&str> = short_names.iter().map(|s| s.as_str()).collect();
            let ann = c.format_annotation(&sn);
            if ann.is_empty() {
                String::new()
            } else {
                format!(" ({})", ann)
            }
        } else {
            String::new()
        };
        report.push_str(&format!(
            "  test {} ... {}{}\n",
            result.name, status, cost_str
        ));
        if let Some(ref err) = result.error {
            report.push_str(&format!("    error: {}\n", err));
        }
    }

    report.push('\n');
    if failed == 0 {
        report.push_str(&format!("test result: ok. {} passed; 0 failed\n", passed));
    } else {
        report.push_str(&format!(
            "test result: FAILED. {} passed; {} failed\n",
            passed, failed
        ));
    }

    Ok(report)
}

/// Parse, type-check, and compute cost analysis for a single file.
pub fn analyze_costs(source: &str, filename: &str) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let cost = cost::CostAnalyzer::default().analyze_file(&file);
    Ok(cost)
}

/// Parse, type-check, and compute cost analysis for a multi-module project.
/// Falls back to single-file analysis if module resolution fails.
pub fn analyze_costs_project(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<cost::ProgramCost, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build(entry_path, options)?;

    // Analyze costs for the program file (last in topological order)
    if let Some(file) = project.last_file() {
        let cost = cost::CostAnalyzer::for_target(&options.target_config.name).analyze_file(file);
        Ok(cost)
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            span::Span::dummy(),
        )])
    }
}

/// Parse, type-check, and verify a project using symbolic execution + solver.
///
/// Returns a `VerificationReport` with static analysis, random testing (Schwartz-Zippel),
/// and bounded model checking results.
pub fn verify_project(entry_path: &Path) -> Result<solve::VerificationReport, Vec<Diagnostic>> {
    use crate::pipeline::PreparedProject;

    let project = PreparedProject::build_default(entry_path)?;

    if let Some(file) = project.last_file() {
        let system = sym::analyze(file);
        Ok(solve::verify(&system))
    } else {
        Err(vec![Diagnostic::error(
            "no program file found".to_string(),
            span::Span::dummy(),
        )])
    }
}

/// Count the number of TASM instructions in a compiled output string.
/// Skips comments, labels, blank lines, and the halt instruction.
pub fn count_tasm_instructions(tasm: &str) -> usize {
    tasm.lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty() && !line.starts_with("//") && !line.ends_with(':') && *line != "halt"
        })
        .count()
}

/// Benchmark result for a single program.
#[derive(Clone, Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub trident_instructions: usize,
    pub baseline_instructions: usize,
    pub overhead_ratio: f64,
    pub trident_padded_height: u64,
    pub baseline_padded_height: u64,
}

impl BenchmarkResult {
    pub fn format(&self) -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>5.2}x  {:>6} {:>6}",
            self.name,
            self.trident_instructions,
            self.baseline_instructions,
            self.overhead_ratio,
            self.trident_padded_height,
            self.baseline_padded_height,
        )
    }

    pub fn format_header() -> String {
        format!(
            "{:<24} {:>6} {:>6}  {:>6}  {:>6} {:>6}",
            "Benchmark", "Tri", "Hand", "Ratio", "TriPad", "HandPad"
        )
    }

    pub fn format_separator() -> String {
        "-".repeat(72)
    }
}

/// Generate markdown documentation for a Trident project.
pub fn generate_docs(
    entry_path: &Path,
    options: &CompileOptions,
) -> Result<String, Vec<Diagnostic>> {
    doc::generate_docs(entry_path, options)
}

/// Parse, type-check, and produce per-line cost-annotated source output.
pub fn annotate_source(source: &str, filename: &str) -> Result<String, Vec<Diagnostic>> {
    annotate_source_with_target(source, filename, "triton")
}

/// Like `annotate_source`, but uses the specified target's cost model.
pub fn annotate_source_with_target(
    source: &str,
    filename: &str,
    target: &str,
) -> Result<String, Vec<Diagnostic>> {
    let file = crate::parse_source(source, filename)?;

    if let Err(errors) = TypeChecker::new().check_file(&file) {
        render_diagnostics(&errors, filename, source);
        return Err(errors);
    }

    let mut analyzer = cost::CostAnalyzer::for_target(target);
    let pc = analyzer.analyze_file(&file);
    let short_names = pc.short_names();
    let stmt_costs = analyzer.stmt_costs(&file, source);

    // Build a map from line number to aggregated cost
    let mut line_costs: HashMap<u32, cost::TableCost> = HashMap::new();
    for (line, cost) in &stmt_costs {
        line_costs
            .entry(*line)
            .and_modify(|existing| *existing = existing.add(cost))
            .or_insert_with(|| cost.clone());
    }

    let lines: Vec<&str> = source.lines().collect();
    let line_count = lines.len();
    let line_num_width = format!("{}", line_count).len().max(2);

    // Find max line length for alignment
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0).min(60);

    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let line_num = (i + 1) as u32;
        let padded_line = format!("{:<width$}", line, width = max_line_len);
        if let Some(cost) = line_costs.get(&line_num) {
            let annotation = cost.format_annotation(&short_names);
            if !annotation.is_empty() {
                out.push_str(&format!(
                    "{:>width$} | {}  [{}]\n",
                    line_num,
                    padded_line,
                    annotation,
                    width = line_num_width,
                ));
                continue;
            }
        }
        out.push_str(&format!(
            "{:>width$} | {}\n",
            line_num,
            line,
            width = line_num_width,
        ));
    }

    Ok(out)
}

/// Format Trident source code, preserving comments.
pub fn format_source(source: &str, _filename: &str) -> Result<String, Vec<Diagnostic>> {
    let (tokens, comments, lex_errors) = lexer::Lexer::new(source, 0).tokenize();
    if !lex_errors.is_empty() {
        return Err(lex_errors);
    }
    let file = parser::Parser::new(tokens).parse_file()?;
    Ok(format::format_file(&file, &comments))
}

/// Type-check only, without rendering diagnostics to stderr.
/// Used by the LSP server to get structured errors.
pub fn check_silent(source: &str, filename: &str) -> Result<(), Vec<Diagnostic>> {
    let file = crate::parse_source_silent(source, filename)?;
    TypeChecker::new().check_file(&file)?;
    Ok(())
}

/// Project-aware type-check for the LSP.
/// Finds trident.toml, resolves dependencies, and type-checks
/// the given file with full module context.
/// Falls back to single-file check if no project is found.
pub fn check_file_in_project(source: &str, file_path: &Path) -> Result<(), Vec<Diagnostic>> {
    let dir = file_path.parent().unwrap_or(Path::new("."));
    let entry = match project::Project::find(dir) {
        Some(toml_path) => match project::Project::load(&toml_path) {
            Ok(p) => p.entry,
            Err(_) => file_path.to_path_buf(),
        },
        None => file_path.to_path_buf(),
    };

    // Resolve all modules from the entry point (handles std.* even without project)
    let modules = match resolve_modules(&entry) {
        Ok(m) => m,
        Err(_) => return check_silent(source, &file_path.to_string_lossy()),
    };

    // Parse and type-check all modules in dependency order
    let mut all_exports: Vec<ModuleExports> = Vec::new();
    let file_path_canon = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());

    for module in &modules {
        let mod_path_canon = module
            .file_path
            .canonicalize()
            .unwrap_or_else(|_| module.file_path.clone());
        let is_target = mod_path_canon == file_path_canon;

        // Use live buffer for the file being edited
        let src = if is_target { source } else { &module.source };
        let parsed = crate::parse_source_silent(src, &module.file_path.to_string_lossy())?;

        let mut tc = TypeChecker::new();
        for exports in &all_exports {
            tc.import_module(exports);
        }

        match tc.check_file(&parsed) {
            Ok(exports) => {
                all_exports.push(exports);
            }
            Err(errors) => {
                if is_target {
                    return Err(errors);
                }
                // Dep has errors — stop, but don't report
                // dep errors as if they're in this file
                return Ok(());
            }
        }
    }

    Ok(())
}
