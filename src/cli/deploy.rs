use std::path::{Path, PathBuf};
use std::process;

use super::{load_dep_dirs, resolve_options};

pub fn cmd_deploy(
    input: PathBuf,
    target: &str,
    profile: &str,
    registry: Option<String>,
    verify: bool,
    dry_run: bool,
) {
    // 1. Resolve input to project or file
    let (project, entry, source_path) = if input.is_dir() {
        // Could be a .deploy/ artifact directory
        if input.join("manifest.json").exists() && input.join("program.tasm").exists() {
            // Pre-packaged artifact — deploy directly from manifest
            let manifest_json = match std::fs::read_to_string(input.join("manifest.json")) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read manifest.json: {}", e);
                    process::exit(1);
                }
            };
            let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);

            if dry_run {
                eprintln!("Dry run — would deploy artifact:");
                eprintln!("  Artifact:  {}", input.display());
                eprintln!("  Registry:  {}", url);
                // Extract name from manifest for display
                for line in manifest_json.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("\"name\"") {
                        eprintln!("  {}", trimmed.trim_end_matches(','));
                    }
                    if trimmed.starts_with("\"program_digest\"") {
                        eprintln!("  {}", trimmed.trim_end_matches(','));
                    }
                }
                return;
            }

            eprintln!("Deploying artifact {} to {}...", input.display(), url);
            deploy_to_registry(&input, &url);
            return;
        }

        // Project directory with trident.toml
        let toml_path = input.join("trident.toml");
        if !toml_path.exists() {
            eprintln!("error: no trident.toml found in '{}'", input.display());
            process::exit(1);
        }
        let project = match trident::project::Project::load(&toml_path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("error: {}", e.message);
                process::exit(1);
            }
        };
        let entry = project.entry.clone();
        let source_path = project.entry.clone();
        (Some(project), entry, source_path)
    } else if input.extension().is_some_and(|e| e == "tri") {
        if let Some(toml_path) =
            trident::project::Project::find(input.parent().unwrap_or(Path::new(".")))
        {
            let project = match trident::project::Project::load(&toml_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {}", e.message);
                    process::exit(1);
                }
            };
            let entry = project.entry.clone();
            (Some(project), entry, input.clone())
        } else {
            (None, input.clone(), input.clone())
        }
    } else {
        eprintln!("error: input must be a .tri file, project directory, or .deploy/ artifact");
        process::exit(1);
    };

    // 2. Resolve target (OS-aware)
    let resolved = match trident::target::ResolvedTarget::resolve(target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}", e.message);
            process::exit(1);
        }
    };

    // 3. Build CompileOptions
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

    // 6. Parse source for function signatures
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
                    eprintln!("error: verification failed — refusing to deploy");
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

    // 9. Package artifact into temp dir
    let output_base = source_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let program_digest =
        trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));

    let target_display = if let Some(ref os) = resolved.os {
        format!("{} ({})", os.name, resolved.vm.name)
    } else {
        resolved.vm.name.clone()
    };

    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);

    // 10. Dry run
    if dry_run {
        eprintln!("Dry run — would deploy:");
        eprintln!("  Name:            {}", name);
        eprintln!("  Version:         {}", version);
        eprintln!("  Target:          {}", target_display);
        eprintln!("  Program digest:  {}", program_digest.to_hex());
        eprintln!("  Padded height:   {}", cost.padded_height);
        eprintln!("  Registry:        {}", url);
        return;
    }

    // 11. Generate artifact
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
    eprintln!("  digest: {}", result.manifest.program_digest);

    // 12. Deploy to registry
    deploy_to_registry(&result.artifact_dir, &url);
}

/// Deploy a packaged artifact directory to a registry server.
fn deploy_to_registry(artifact_dir: &Path, url: &str) {
    eprintln!("Deploying to {}...", url);
    let client = trident::registry::RegistryClient::new(url);
    match client.health() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: registry at {} is not healthy", url);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: cannot reach registry at {}: {}", url, e);
            process::exit(1);
        }
    }

    // Read manifest to get program info
    let manifest_path = artifact_dir.join("manifest.json");
    let tasm_path = artifact_dir.join("program.tasm");

    if !manifest_path.exists() || !tasm_path.exists() {
        eprintln!(
            "error: artifact directory '{}' missing manifest.json or program.tasm",
            artifact_dir.display()
        );
        process::exit(1);
    }

    let tasm = match std::fs::read_to_string(&tasm_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read program.tasm: {}", e);
            process::exit(1);
        }
    };

    // Parse the TASM as a pseudo-source to get function definitions for UCM.
    // Since we already have manifest.json, we use the original source if available.
    // Fall back to publishing just the compiled artifact.
    let source_path = artifact_dir.parent().and_then(|parent| {
        // Look for a .tri file next to the .deploy/ directory
        let stem = artifact_dir
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".deploy");
        let tri_file = parent.join(format!("{}.tri", stem));
        if tri_file.exists() {
            Some(tri_file)
        } else {
            None
        }
    });

    if let Some(source_file) = source_path {
        let source = match std::fs::read_to_string(&source_file) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("warning: cannot read source file, publishing artifact only");
                publish_artifact_only(&client, &tasm);
                return;
            }
        };
        let filename = source_file.to_string_lossy().to_string();
        match trident::parse_source_silent(&source, &filename) {
            Ok(file) => {
                let mut cb = match trident::ucm::Codebase::open() {
                    Ok(cb) => cb,
                    Err(e) => {
                        eprintln!("error: cannot open codebase: {}", e);
                        process::exit(1);
                    }
                };
                cb.add_file(&file);
                if let Err(e) = cb.save() {
                    eprintln!("error: cannot save codebase: {}", e);
                    process::exit(1);
                }
                match trident::registry::publish_codebase(&cb, &client, &[]) {
                    Ok(results) => {
                        let created = results.iter().filter(|r| r.created).count();
                        eprintln!(
                            "Deployed: {} definitions ({} new) to {}",
                            results.len(),
                            created,
                            url
                        );
                    }
                    Err(e) => {
                        eprintln!("error: deploy failed: {}", e);
                        process::exit(1);
                    }
                }
            }
            Err(_) => {
                eprintln!("warning: cannot parse source, publishing artifact only");
                publish_artifact_only(&client, &tasm);
            }
        }
    } else {
        publish_artifact_only(&client, &tasm);
    }
}

/// Publish just the compiled TASM when source is unavailable.
fn publish_artifact_only(client: &trident::registry::RegistryClient, tasm: &str) {
    // Create a minimal codebase entry for the compiled artifact
    let hash = trident::hash::ContentHash(trident::poseidon2::hash_bytes(tasm.as_bytes()));
    eprintln!("Publishing artifact (digest: {})...", hash.to_hex());
    // Use the registry's raw definition publish endpoint
    let cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };
    match trident::registry::publish_codebase(&cb, client, &[]) {
        Ok(results) => {
            let created = results.iter().filter(|r| r.created).count();
            eprintln!("Deployed: {} definitions ({} new)", results.len(), created);
        }
        Err(e) => {
            eprintln!("error: deploy failed: {}", e);
            process::exit(1);
        }
    }
}
