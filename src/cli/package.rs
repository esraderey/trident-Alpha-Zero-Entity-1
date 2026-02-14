use std::path::{Path, PathBuf};
use std::process;

use super::{load_dep_dirs, resolve_input, resolve_options};

pub fn cmd_package(
    input: PathBuf,
    output: Option<PathBuf>,
    target: &str,
    profile: &str,
    verify: bool,
    dry_run: bool,
) {
    // 1. Resolve input to project or file
    let ri = resolve_input(&input);
    let project = ri.project;
    let entry = ri.entry;
    let source_path = entry.clone();

    // 2. Resolve target (OS-aware)
    let resolved = match trident::target::ResolvedTarget::resolve(target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    // 3. Build CompileOptions using the resolved VM config
    let mut options = resolve_options(&resolved.vm.name, profile, project.as_ref());
    options.target_config = resolved.vm.clone();
    if let Some(ref proj) = project {
        options.dep_dirs = load_dep_dirs(proj);
    }

    // 4. Compile
    eprintln!("Compiling {}...", source_path.display());
    let tasm = match trident::compile_project_with_options(&entry, &options) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("error: compilation failed");
            process::exit(1);
        }
    };

    // 5. Cost analysis
    let cost = match trident::analyze_costs_project(&entry, &options) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("warning: cost analysis failed, using zeros");
            trident::cost::ProgramCost {
                program_name: String::new(),
                functions: Vec::new(),
                total: trident::cost::TableCost::ZERO,
                attestation_hash_rows: 0,
                padded_height: 0,
                estimated_proving_secs: 0.0,
                loop_bound_waste: Vec::new(),
            }
        }
    };

    // 6. Parse source for function signatures and hashes
    let source = std::fs::read_to_string(&source_path).unwrap_or_default();
    let filename = source_path.to_string_lossy().to_string();
    let file = match trident::parse_source_silent(&source, &filename) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("error: cannot parse source for manifest");
            process::exit(1);
        }
    };

    // 7. Determine name and version
    let (name, version) = if let Some(ref proj) = project {
        (proj.name.clone(), proj.version.clone())
    } else {
        let stem = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("program")
            .to_string();
        (stem, "0.1.0".to_string())
    };

    // 8. Optional verification
    if verify {
        eprintln!("Verifying {}...", source_path.display());
        match trident::verify_project(&entry) {
            Ok(report) => {
                if !report.is_safe() {
                    eprintln!("error: verification failed");
                    eprintln!("{}", report.format_report());
                    process::exit(1);
                }
                eprintln!("Verification: OK");
            }
            Err(_) => {
                eprintln!("error: verification failed");
                process::exit(1);
            }
        }
    }

    // 9. Determine output base directory
    let output_base = output.unwrap_or_else(|| {
        if let Some(ref proj) = project {
            proj.root_dir.clone()
        } else {
            source_path.parent().unwrap_or(Path::new(".")).to_path_buf()
        }
    });

    // 10. Compute program digest for display / dry run
    let program_digest =
        trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));

    // Target display string
    let target_display = if let Some(ref os) = resolved.os {
        format!("{} ({})", os.name, resolved.vm.name)
    } else {
        resolved.vm.name.clone()
    };

    // 11. Dry run
    if dry_run {
        eprintln!("Dry run — would package:");
        eprintln!("  Name:            {}", name);
        eprintln!("  Version:         {}", version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", cost.padded_height);
        eprintln!(
            "  Artifact:        {}/{}.deploy/",
            output_base.display(),
            name
        );
        return;
    }

    // 12. Generate artifact
    let result = match trident::artifact::generate_artifact(
        &name,
        &version,
        &tasm,
        &file,
        &cost,
        &resolved.vm,
        resolved.os.as_ref(),
        &output_base,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Packaged -> {}", result.artifact_dir.display());
    eprintln!("  program.tasm:   {}", result.tasm_path.display());
    eprintln!("  manifest.json:  {}", result.manifest_path.display());
    eprintln!("  digest:         {}", result.manifest.program_digest);
    eprintln!("  padded height:  {}", result.manifest.cost.padded_height);
    eprintln!("  target:         {}", target_display);
}
