use std::path::PathBuf;
use std::process;

use super::collect_tri_files;
use crate::RegistryAction;

pub fn cmd_registry(action: RegistryAction) {
    match action {
        RegistryAction::Publish {
            registry,
            tag,
            input,
        } => cmd_registry_publish(registry, tag, input),
        RegistryAction::Pull { name, registry } => cmd_registry_pull(name, registry),
        RegistryAction::Search {
            query,
            registry,
            r#type,
            tag,
            verified: _,
        } => cmd_registry_search(query, registry, r#type, tag),
    }
}

fn cmd_registry_publish(registry: Option<String>, tags: Vec<String>, input: Option<PathBuf>) {
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);

    // Check health first.
    match client.health() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("error: registry at {} is not healthy", url);
            process::exit(1);
        }
        Err(e) => {
            eprintln!("error: cannot connect to registry at {}: {}", url, e);
            process::exit(1);
        }
    }

    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    // If input is provided, add to UCM first.
    if let Some(ref input_path) = input {
        let files = if input_path.is_dir() {
            collect_tri_files(input_path)
        } else if input_path.extension().is_some_and(|e| e == "tri") {
            vec![input_path.clone()]
        } else {
            eprintln!("error: input must be a .tri file or directory");
            process::exit(1);
        };

        for file_path in &files {
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read '{}': {}", file_path.display(), e);
                    continue;
                }
            };
            let filename = file_path.to_string_lossy().to_string();
            if let Ok(file) = trident::parse_source_silent(&source, &filename) {
                cb.add_file(&file);
            }
        }
        if let Err(e) = cb.save() {
            eprintln!("error: cannot save codebase: {}", e);
        }
    }

    eprintln!("Publishing to {}...", url);
    match trident::registry::publish_codebase(&cb, &client, &tags) {
        Ok(results) => {
            let created = results.iter().filter(|r| r.created).count();
            let existing = results.len() - created;
            let named = results.iter().filter(|r| r.name_bound).count();
            eprintln!(
                "Published: {} new, {} existing, {} names bound",
                created, existing, named
            );
        }
        Err(e) => {
            eprintln!("error: publish failed: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_registry_pull(name: String, registry: Option<String>) {
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);

    let mut cb = match trident::ucm::Codebase::open() {
        Ok(cb) => cb,
        Err(e) => {
            eprintln!("error: cannot open codebase: {}", e);
            process::exit(1);
        }
    };

    eprintln!("Pulling '{}' from {}...", name, url);
    match trident::registry::pull_into_codebase(&mut cb, &client, &name) {
        Ok(result) => {
            eprintln!("Pulled: {} ({})", name, &result.hash[..16]);
            eprintln!("  Module: {}", result.module);
            if !result.params.is_empty() {
                let params: Vec<String> = result
                    .params
                    .iter()
                    .map(|(n, t)| format!("{}: {}", n, t))
                    .collect();
                eprintln!("  Params: {}", params.join(", "));
            }
            if let Some(ref ret) = result.return_ty {
                eprintln!("  Returns: {}", ret);
            }
            if !result.dependencies.is_empty() {
                eprintln!("  Dependencies: {}", result.dependencies.len());
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_registry_search(query: String, registry: Option<String>, by_type: bool, by_tag: bool) {
    let url = registry.unwrap_or_else(trident::registry::RegistryClient::default_url);
    let client = trident::registry::RegistryClient::new(&url);

    let results = if by_type {
        client.search_by_type(&query)
    } else if by_tag {
        client.search_by_tag(&query)
    } else {
        client.search(&query)
    };

    match results {
        Ok(results) => {
            if results.is_empty() {
                eprintln!("No results for '{}'", query);
                return;
            }
            for r in &results {
                let verified = if r.verified { " [verified]" } else { "" };
                let tags = if r.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", r.tags.join(", "))
                };
                println!(
                    "  {}  {}  {}{}{}",
                    &r.hash[..16],
                    r.name,
                    r.signature,
                    verified,
                    tags
                );
            }
            eprintln!("\n{} results", results.len());
        }
        Err(e) => {
            eprintln!("error: {}", e);
            process::exit(1);
        }
    }
}
