//! Global Registry — HTTP API server + client for hash-keyed definitions.
//!
//! Provides a network-accessible registry that mirrors the local UCM codebase
//! format. Definitions are stored by content hash (BLAKE3), names are metadata.
//!
//! ## Protocol
//!
//! Simple HTTP/1.1 JSON API:
//!
//! ```text
//! GET  /api/v1/definitions/<hash>          — definition JSON
//! GET  /api/v1/definitions?prefix=<hex>    — definition by prefix
//! POST /api/v1/definitions                 — publish definition
//! GET  /api/v1/search?q=<query>            — search definitions
//! GET  /api/v1/search?type=<sig>           — search by type signature
//! GET  /api/v1/search?tag=<tag>            — search by tag
//! GET  /api/v1/stats                       — registry statistics
//! GET  /api/v1/names                       — list all names
//! GET  /api/v1/names/<name>                — resolve name to hash
//! POST /api/v1/names/<name>                — bind name to hash
//! GET  /api/v1/deps/<hash>                 — transitive dependencies
//! GET  /health                             — health check
//! ```
//!
//! ## Storage
//!
//! Reuses the UCM `Codebase` format on the server side.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::hash::ContentHash;
use crate::ucm::{Codebase, Definition};

// ─── Registry Configuration ───────────────────────────────────────

/// Registry server configuration.
pub struct RegistryConfig {
    /// Bind address (e.g. "127.0.0.1:8090").
    pub bind_addr: String,
    /// Storage directory for the registry codebase.
    pub storage_dir: PathBuf,
    /// Maximum request body size in bytes.
    pub max_body_size: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8090".to_string(),
            storage_dir: default_registry_dir(),
            max_body_size: 1024 * 1024, // 1 MB
        }
    }
}

fn default_registry_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TRIDENT_REGISTRY_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".trident").join("registry"))
        .unwrap_or_else(|_| PathBuf::from(".trident-registry"))
}

// ─── Published Definition (wire format) ───────────────────────────

/// A definition as published to the registry (JSON wire format).
#[derive(Clone, Debug)]
pub struct PublishedDefinition {
    /// Content hash (hex).
    pub hash: String,
    /// Function source code.
    pub source: String,
    /// Module name.
    pub module: String,
    /// Is it public?
    pub is_pub: bool,
    /// Parameters: [(name, type)].
    pub params: Vec<(String, String)>,
    /// Return type (if any).
    pub return_ty: Option<String>,
    /// Dependencies: hex hashes of called functions.
    pub dependencies: Vec<String>,
    /// Preconditions.
    pub requires: Vec<String>,
    /// Postconditions.
    pub ensures: Vec<String>,
    /// Name binding (if any).
    pub name: Option<String>,
    /// Tags for search.
    pub tags: Vec<String>,
    /// Verification status.
    pub verified: bool,
    /// Verification certificate (opaque string, if available).
    pub verification_cert: Option<String>,
}

/// Search result entry.
#[derive(Clone, Debug)]
pub struct SearchResult {
    pub name: String,
    pub hash: String,
    pub module: String,
    pub signature: String,
    pub verified: bool,
    pub tags: Vec<String>,
}

/// Result of a publish operation.
#[derive(Clone, Debug)]
pub struct PublishResult {
    pub hash: String,
    pub created: bool,
    pub name_bound: bool,
}

/// Result of a pull operation.
#[derive(Clone, Debug)]
pub struct PullResult {
    pub hash: String,
    pub source: String,
    pub module: String,
    pub params: Vec<(String, String)>,
    pub return_ty: Option<String>,
    pub dependencies: Vec<String>,
    pub requires: Vec<String>,
    pub ensures: Vec<String>,
}

// ─── Registry Metadata ────────────────────────────────────────────

/// Extended metadata stored alongside definitions in the registry.
/// Persisted as <hash>.meta files.
#[derive(Clone)]
struct RegistryMeta {
    tags: Vec<String>,
    verified: bool,
    verification_cert: Option<String>,
    downloads: u64,
    published_at: u64,
    publisher: Option<String>,
}

impl RegistryMeta {
    fn new() -> Self {
        Self {
            tags: Vec::new(),
            verified: false,
            verification_cert: None,
            downloads: 0,
            published_at: unix_timestamp(),
            publisher: None,
        }
    }

    fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("tags=");
        out.push_str(&self.tags.join(","));
        out.push('\n');
        out.push_str("verified=");
        out.push_str(if self.verified { "true" } else { "false" });
        out.push('\n');
        if let Some(ref cert) = self.verification_cert {
            out.push_str("verification_cert=");
            out.push_str(cert);
            out.push('\n');
        }
        out.push_str("downloads=");
        out.push_str(&self.downloads.to_string());
        out.push('\n');
        out.push_str("published_at=");
        out.push_str(&self.published_at.to_string());
        out.push('\n');
        if let Some(ref pub_by) = self.publisher {
            out.push_str("publisher=");
            out.push_str(pub_by);
            out.push('\n');
        }
        out
    }

    fn deserialize(text: &str) -> Self {
        let mut meta = Self::new();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "tags" => {
                        meta.tags = if value.is_empty() {
                            Vec::new()
                        } else {
                            value.split(',').map(|s| s.trim().to_string()).collect()
                        };
                    }
                    "verified" => meta.verified = value.trim() == "true",
                    "verification_cert" => {
                        meta.verification_cert = Some(value.to_string());
                    }
                    "downloads" => meta.downloads = value.trim().parse().unwrap_or(0),
                    "published_at" => meta.published_at = value.trim().parse().unwrap_or(0),
                    "publisher" => meta.publisher = Some(value.to_string()),
                    _ => {}
                }
            }
        }
        meta
    }
}

// ─── Registry Server ──────────────────────────────────────────────

/// Extended registry state: Codebase + metadata.
struct RegistryState {
    codebase: Codebase,
    metadata: HashMap<String, RegistryMeta>, // hash hex -> metadata
    storage_dir: PathBuf,
}

impl RegistryState {
    fn load(storage_dir: &Path) -> std::io::Result<Self> {
        let cb = Codebase::open_at(storage_dir)?;
        let mut metadata = HashMap::new();

        // Load metadata files.
        let meta_dir = storage_dir.join("meta");
        if meta_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&meta_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("meta") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                metadata
                                    .insert(stem.to_string(), RegistryMeta::deserialize(&content));
                            }
                        }
                    }
                }
            }
        }

        Ok(Self {
            codebase: cb,
            metadata,
            storage_dir: storage_dir.to_path_buf(),
        })
    }

    fn save(&self) -> std::io::Result<()> {
        self.codebase.save()?;

        // Save metadata files.
        let meta_dir = self.storage_dir.join("meta");
        std::fs::create_dir_all(&meta_dir)?;
        for (hash_hex, meta) in &self.metadata {
            let path = meta_dir.join(format!("{}.meta", hash_hex));
            std::fs::write(&path, meta.serialize())?;
        }

        Ok(())
    }

    fn save_meta(&self, hash_hex: &str) -> std::io::Result<()> {
        if let Some(meta) = self.metadata.get(hash_hex) {
            let meta_dir = self.storage_dir.join("meta");
            std::fs::create_dir_all(&meta_dir)?;
            let path = meta_dir.join(format!("{}.meta", hash_hex));
            std::fs::write(&path, meta.serialize())?;
        }
        Ok(())
    }
}

/// Run the registry HTTP server (blocking).
pub fn run_server(config: &RegistryConfig) -> std::io::Result<()> {
    let state = Arc::new(Mutex::new(RegistryState::load(&config.storage_dir)?));

    let listener = TcpListener::bind(&config.bind_addr)?;
    eprintln!("Trident Registry listening on http://{}", config.bind_addr);
    eprintln!("Storage: {}", config.storage_dir.display());

    let max_body = config.max_body_size;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, &state, max_body) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Accept error: {}", e);
            }
        }
    }

    Ok(())
}

fn handle_connection(
    stream: TcpStream,
    state: &Arc<Mutex<RegistryState>>,
    max_body: usize,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;

    let mut reader = BufReader::new(&stream);

    // Read request line.
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let request_line = request_line.trim().to_string();

    // Parse method and path.
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return send_response(&stream, 400, "Bad Request", "{\"error\":\"bad request\"}");
    }
    let method = parts[0];
    let full_path = parts[1];

    // Read headers.
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }

    // Read body if Content-Length is set.
    let body = if let Some(len_str) = headers.get("content-length") {
        let len: usize = len_str.parse().unwrap_or(0);
        if len > max_body {
            return send_response(
                &stream,
                413,
                "Payload Too Large",
                "{\"error\":\"body too large\"}",
            );
        }
        let mut body = vec![0u8; len];
        std::io::Read::read_exact(&mut reader, &mut body)?;
        String::from_utf8(body).unwrap_or_default()
    } else {
        String::new()
    };

    // Split path and query string.
    let (path, query) = full_path.split_once('?').unwrap_or((full_path, ""));
    let query_params = parse_query(query);

    // Route the request.
    let response = route_request(method, path, &query_params, &body, state);

    send_response(
        &stream,
        response.status,
        &response.status_text,
        &response.body,
    )
}

struct HttpResponse {
    status: u16,
    status_text: String,
    body: String,
}

fn route_request(
    method: &str,
    path: &str,
    query: &HashMap<String, String>,
    body: &str,
    state: &Arc<Mutex<RegistryState>>,
) -> HttpResponse {
    // Validate path: no path traversal.
    if path.contains("..") {
        return json_response(400, "Bad Request", "{\"error\":\"invalid path\"}");
    }

    match (method, path) {
        ("GET", "/health") => json_response(200, "OK", "{\"status\":\"ok\",\"version\":\"0.1.0\"}"),

        ("GET", "/api/v1/stats") => handle_stats(state),

        ("GET", "/api/v1/names") => handle_list_names(state),

        ("GET", p) if p.starts_with("/api/v1/names/") => {
            let name = &p["/api/v1/names/".len()..];
            handle_resolve_name(name, state)
        }

        ("POST", p) if p.starts_with("/api/v1/names/") => {
            let name = &p["/api/v1/names/".len()..];
            handle_bind_name(name, body, state)
        }

        ("GET", p) if p.starts_with("/api/v1/definitions/") => {
            let hash_str = &p["/api/v1/definitions/".len()..];
            handle_get_definition(hash_str, state)
        }

        ("GET", "/api/v1/definitions") => {
            if let Some(prefix) = query.get("prefix") {
                handle_get_definition_by_prefix(prefix, state)
            } else {
                json_response(
                    400,
                    "Bad Request",
                    "{\"error\":\"missing prefix parameter\"}",
                )
            }
        }

        ("POST", "/api/v1/definitions") => handle_publish(body, state),

        ("GET", "/api/v1/search") => handle_search(query, state),

        ("GET", p) if p.starts_with("/api/v1/deps/") => {
            let hash_str = &p["/api/v1/deps/".len()..];
            handle_deps(hash_str, state)
        }

        _ => json_response(404, "Not Found", "{\"error\":\"not found\"}"),
    }
}

// ─── Request Handlers ─────────────────────────────────────────────

fn handle_stats(state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    let guard = state.lock().unwrap();
    let stats = guard.codebase.stats();
    let meta_count = guard.metadata.len();
    let verified_count = guard.metadata.values().filter(|m| m.verified).count();
    json_response(
        200,
        "OK",
        &format!(
            "{{\"definitions\":{},\"names\":{},\"total_source_bytes\":{},\"metadata\":{},\"verified\":{}}}",
            stats.definitions, stats.names, stats.total_source_bytes, meta_count, verified_count
        ),
    )
}

fn handle_list_names(state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    let guard = state.lock().unwrap();
    let names = guard.codebase.list_names();
    let mut json = String::from("{\"names\":[");
    for (i, (name, hash)) in names.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"name\":{},\"hash\":\"{}\"}}",
            json_escape(name),
            hash.to_hex()
        ));
    }
    json.push_str("]}");
    json_response(200, "OK", &json)
}

fn handle_resolve_name(name: &str, state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    if !is_valid_name(name) {
        return json_response(400, "Bad Request", "{\"error\":\"invalid name\"}");
    }

    let guard = state.lock().unwrap();
    if let Some(def) = guard.codebase.lookup(name) {
        let hash = guard
            .codebase
            .list_names()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, h)| h.to_hex())
            .unwrap_or_default();
        let meta = guard.metadata.get(&hash);
        json_response(200, "OK", &format_definition_json(&hash, name, def, meta))
    } else {
        json_response(
            404,
            "Not Found",
            &format!(
                "{{\"error\":\"name '{}' not found\"}}",
                json_escape_inner(name)
            ),
        )
    }
}

fn handle_bind_name(name: &str, body: &str, state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    if !is_valid_name(name) {
        return json_response(400, "Bad Request", "{\"error\":\"invalid name\"}");
    }

    let hash_hex = extract_json_string(body, "hash");
    if hash_hex.is_empty() || hash_hex.len() != 64 {
        return json_response(400, "Bad Request", "{\"error\":\"invalid hash\"}");
    }

    let hash = match parse_hex_hash(&hash_hex) {
        Some(h) => h,
        None => return json_response(400, "Bad Request", "{\"error\":\"invalid hash format\"}"),
    };

    let mut guard = state.lock().unwrap();
    if guard.codebase.lookup_hash(&hash).is_none() {
        return json_response(
            404,
            "Not Found",
            "{\"error\":\"definition not found for given hash\"}",
        );
    }

    let existing = guard
        .codebase
        .list_names()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, h)| *h);

    if let Some(existing_hash) = existing {
        if existing_hash == hash {
            return json_response(
                200,
                "OK",
                &format!(
                    "{{\"name\":{},\"hash\":\"{}\",\"status\":\"unchanged\"}}",
                    json_escape(name),
                    hash_hex
                ),
            );
        }
        return json_response(
            409,
            "Conflict",
            &format!(
                "{{\"error\":\"name '{}' already bound to {}\"}}",
                json_escape_inner(name),
                existing_hash.to_hex()
            ),
        );
    }

    let existing_name = guard.codebase.names_for_hash(&hash);
    if let Some(source_name) = existing_name.first() {
        let source = source_name.to_string();
        match guard.codebase.alias(&source, name) {
            Ok(()) => {
                let _ = guard.save();
                json_response(
                    200,
                    "OK",
                    &format!(
                        "{{\"name\":{},\"hash\":\"{}\",\"status\":\"bound\"}}",
                        json_escape(name),
                        hash_hex
                    ),
                )
            }
            Err(e) => json_response(
                500,
                "Internal Server Error",
                &format!("{{\"error\":{}}}", json_escape(&e)),
            ),
        }
    } else {
        json_response(
            500,
            "Internal Server Error",
            "{\"error\":\"definition has no names\"}",
        )
    }
}

fn handle_get_definition(hash_str: &str, state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    if hash_str.len() != 64 || !hash_str.chars().all(|c| c.is_ascii_hexdigit()) {
        return json_response(400, "Bad Request", "{\"error\":\"invalid hash format\"}");
    }

    let hash = match parse_hex_hash(hash_str) {
        Some(h) => h,
        None => return json_response(400, "Bad Request", "{\"error\":\"invalid hash\"}"),
    };

    let mut guard = state.lock().unwrap();
    if let Some(def) = guard.codebase.lookup_hash(&hash) {
        let def = def.clone();
        let name = guard
            .codebase
            .names_for_hash(&hash)
            .first()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let meta_snapshot = guard.metadata.get(hash_str).cloned();

        // Increment download count.
        let meta_entry = guard
            .metadata
            .entry(hash_str.to_string())
            .or_insert_with(RegistryMeta::new);
        meta_entry.downloads = meta_entry.downloads.saturating_add(1);
        let _ = guard.save_meta(hash_str);

        let json = format_definition_json(hash_str, &name, &def, meta_snapshot.as_ref());
        json_response(200, "OK", &json)
    } else {
        json_response(404, "Not Found", "{\"error\":\"definition not found\"}")
    }
}

fn handle_get_definition_by_prefix(
    prefix: &str,
    state: &Arc<Mutex<RegistryState>>,
) -> HttpResponse {
    if prefix.len() < 4 || prefix.len() > 64 {
        return json_response(
            400,
            "Bad Request",
            "{\"error\":\"prefix must be 4-64 hex chars\"}",
        );
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c.is_ascii_alphanumeric())
    {
        return json_response(400, "Bad Request", "{\"error\":\"invalid prefix format\"}");
    }

    let guard = state.lock().unwrap();
    if let Some((hash, def)) = guard.codebase.lookup_by_prefix(prefix) {
        let hash_hex = hash.to_hex();
        let name = guard
            .codebase
            .names_for_hash(hash)
            .first()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let meta = guard.metadata.get(&hash_hex);
        json_response(
            200,
            "OK",
            &format_definition_json(&hash_hex, &name, def, meta),
        )
    } else {
        json_response(
            404,
            "Not Found",
            "{\"error\":\"no definition matching prefix\"}",
        )
    }
}

fn handle_publish(body: &str, state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    let pub_def = match parse_publish_body(body) {
        Ok(d) => d,
        Err(e) => {
            return json_response(
                400,
                "Bad Request",
                &format!("{{\"error\":{}}}", json_escape(&e)),
            )
        }
    };

    let hash = match parse_hex_hash(&pub_def.hash) {
        Some(h) => h,
        None => return json_response(400, "Bad Request", "{\"error\":\"invalid hash\"}"),
    };

    let mut guard = state.lock().unwrap();

    let already_exists = guard.codebase.lookup_hash(&hash).is_some();

    if !already_exists {
        let deps: Vec<ContentHash> = pub_def
            .dependencies
            .iter()
            .filter_map(|h| parse_hex_hash(h))
            .collect();

        let def = Definition {
            source: pub_def.source.clone(),
            module: pub_def.module.clone(),
            is_pub: pub_def.is_pub,
            params: pub_def.params.clone(),
            return_ty: pub_def.return_ty.clone(),
            dependencies: deps,
            requires: pub_def.requires.clone(),
            ensures: pub_def.ensures.clone(),
            first_seen: unix_timestamp(),
        };

        guard.codebase.store_definition(hash, def);
    }

    let mut name_bound = false;
    if let Some(ref name) = pub_def.name {
        if is_valid_name(name) {
            let existing = guard
                .codebase
                .list_names()
                .into_iter()
                .find(|(n, _)| *n == name.as_str())
                .map(|(_, h)| *h);
            if existing.is_none() {
                guard.codebase.bind_name(name, hash);
                name_bound = true;
            }
        }
    }

    // Store/update metadata.
    let meta = guard
        .metadata
        .entry(pub_def.hash.clone())
        .or_insert_with(RegistryMeta::new);
    if !pub_def.tags.is_empty() {
        meta.tags = pub_def.tags.clone();
    }
    if pub_def.verified {
        meta.verified = true;
    }
    if let Some(ref cert) = pub_def.verification_cert {
        meta.verification_cert = Some(cert.clone());
    }

    if let Err(e) = guard.save() {
        return json_response(
            500,
            "Internal Server Error",
            &format!("{{\"error\":{}}}", json_escape(&e.to_string())),
        );
    }

    json_response(
        if already_exists { 200 } else { 201 },
        if already_exists { "OK" } else { "Created" },
        &format!(
            "{{\"hash\":\"{}\",\"created\":{},\"name_bound\":{}}}",
            pub_def.hash, !already_exists, name_bound,
        ),
    )
}

fn handle_search(
    query: &HashMap<String, String>,
    state: &Arc<Mutex<RegistryState>>,
) -> HttpResponse {
    let guard = state.lock().unwrap();
    let mut results: Vec<SearchResult> = Vec::new();

    let names = guard.codebase.list_names();

    let q = query.get("q").map(|s| s.to_lowercase());
    let type_sig = query.get("type").map(|s| s.to_lowercase());
    let tag_filter = query.get("tag").map(|s| s.to_lowercase());
    let verified_only = query.get("verified").map(|s| s == "true").unwrap_or(false);
    let limit: usize = query
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50)
        .min(200);

    for (name, hash) in &names {
        let hash_hex = hash.to_hex();
        let def = match guard.codebase.lookup_hash(hash) {
            Some(d) => d,
            None => continue,
        };
        let meta = guard.metadata.get(&hash_hex);
        let is_verified = meta.map(|m| m.verified).unwrap_or(false);
        let tags: Vec<String> = meta.map(|m| m.tags.clone()).unwrap_or_default();

        if verified_only && !is_verified {
            continue;
        }

        if let Some(ref tag) = tag_filter {
            if !tags.iter().any(|t| t.to_lowercase().contains(tag)) {
                continue;
            }
        }

        let sig = format_type_signature(def);

        if let Some(ref q) = q {
            let name_lower = name.to_lowercase();
            let module_lower = def.module.to_lowercase();
            if !name_lower.contains(q)
                && !module_lower.contains(q)
                && !sig.to_lowercase().contains(q)
            {
                continue;
            }
        }

        if let Some(ref type_q) = type_sig {
            if !sig.to_lowercase().contains(type_q) {
                continue;
            }
        }

        results.push(SearchResult {
            name: name.to_string(),
            hash: hash_hex,
            module: def.module.clone(),
            signature: sig,
            verified: is_verified,
            tags,
        });

        if results.len() >= limit {
            break;
        }
    }

    let mut json = String::from("{\"results\":[");
    for (i, r) in results.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let tags_json: Vec<String> = r.tags.iter().map(|t| json_escape(t)).collect();
        json.push_str(&format!(
            "{{\"name\":{},\"hash\":\"{}\",\"module\":{},\"signature\":{},\"verified\":{},\"tags\":[{}]}}",
            json_escape(&r.name),
            r.hash,
            json_escape(&r.module),
            json_escape(&r.signature),
            r.verified,
            tags_json.join(","),
        ));
    }
    json.push_str(&format!("],\"count\":{}}}", results.len()));
    json_response(200, "OK", &json)
}

fn handle_deps(hash_str: &str, state: &Arc<Mutex<RegistryState>>) -> HttpResponse {
    let hash = match parse_hex_hash(hash_str) {
        Some(h) => h,
        None => return json_response(400, "Bad Request", "{\"error\":\"invalid hash\"}"),
    };

    let guard = state.lock().unwrap();
    let deps = guard.codebase.dependencies(&hash);

    let mut json = String::from("{\"dependencies\":[");
    for (i, (name, dep_hash)) in deps.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"name\":{},\"hash\":\"{}\"}}",
            json_escape(name),
            dep_hash.to_hex(),
        ));
    }
    json.push_str("]}");
    json_response(200, "OK", &json)
}

// ─── Registry Client ──────────────────────────────────────────────

/// Client for interacting with a remote Trident registry.
pub struct RegistryClient {
    base_url: String,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(url: &str) -> Self {
        Self {
            base_url: url.trim_end_matches('/').to_string(),
        }
    }

    /// Get the default registry URL from environment or config.
    pub fn default_url() -> String {
        std::env::var("TRIDENT_REGISTRY_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string())
    }

    /// Publish a definition to the registry.
    pub fn publish(&self, def: &PublishedDefinition) -> Result<PublishResult, String> {
        let body = format_publish_json(def);
        let response = self.http_post("/api/v1/definitions", &body)?;

        if response.status >= 400 {
            return Err(format!(
                "publish failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(PublishResult {
            hash: extract_json_string(&response.body, "hash"),
            created: extract_json_bool(&response.body, "created"),
            name_bound: extract_json_bool(&response.body, "name_bound"),
        })
    }

    /// Pull a definition from the registry by hash.
    pub fn pull(&self, hash: &str) -> Result<PullResult, String> {
        let path = format!("/api/v1/definitions/{}", hash);
        let response = self.http_get(&path)?;

        if response.status == 404 {
            return Err(format!("definition {} not found in registry", hash));
        }
        if response.status >= 400 {
            return Err(format!(
                "pull failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_pull_response(&response.body))
    }

    /// Pull a definition by name.
    pub fn pull_by_name(&self, name: &str) -> Result<PullResult, String> {
        let path = format!("/api/v1/names/{}", name);
        let response = self.http_get(&path)?;

        if response.status == 404 {
            return Err(format!("name '{}' not found in registry", name));
        }
        if response.status >= 400 {
            return Err(format!(
                "pull failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_pull_response(&response.body))
    }

    /// Search the registry.
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?q={}", url_encode(query));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Search by type signature.
    pub fn search_by_type(&self, type_sig: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?type={}", url_encode(type_sig));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Search by tag.
    pub fn search_by_tag(&self, tag: &str) -> Result<Vec<SearchResult>, String> {
        let path = format!("/api/v1/search?tag={}", url_encode(tag));
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "search failed ({}): {}",
                response.status, response.body
            ));
        }

        Ok(parse_search_response(&response.body))
    }

    /// Check registry health.
    pub fn health(&self) -> Result<bool, String> {
        let response = self.http_get("/health")?;
        Ok(response.status == 200)
    }

    /// Get registry statistics.
    pub fn stats(&self) -> Result<String, String> {
        let response = self.http_get("/api/v1/stats")?;
        if response.status >= 400 {
            return Err(format!(
                "stats failed ({}): {}",
                response.status, response.body
            ));
        }
        Ok(response.body)
    }

    /// Get transitive dependencies.
    pub fn deps(&self, hash: &str) -> Result<Vec<(String, String)>, String> {
        let path = format!("/api/v1/deps/{}", hash);
        let response = self.http_get(&path)?;

        if response.status >= 400 {
            return Err(format!(
                "deps failed ({}): {}",
                response.status, response.body
            ));
        }

        let mut result = Vec::new();
        let body = &response.body;
        let deps_start = body.find('[').unwrap_or(body.len());
        let deps_end = body.rfind(']').unwrap_or(body.len());
        if deps_start < deps_end {
            let deps_str = &body[deps_start + 1..deps_end];
            for obj in deps_str.split("},") {
                let name = extract_json_string(obj, "name");
                let hash = extract_json_string(obj, "hash");
                if !hash.is_empty() {
                    result.push((name, hash));
                }
            }
        }

        Ok(result)
    }

    // ─── HTTP Transport ───────────────────────────────────────

    fn http_get(&self, path: &str) -> Result<ClientResponse, String> {
        let (host, port, scheme_host) = parse_url(&self.base_url)?;
        let addr = format!("{}:{}", host, port);

        let stream =
            TcpStream::connect(&addr).map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("set timeout: {}", e))?;

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: trident/0.1\r\n\r\n",
            path, scheme_host,
        );

        (&stream)
            .write_all(request.as_bytes())
            .map_err(|e| format!("write request: {}", e))?;

        read_response(&stream)
    }

    fn http_post(&self, path: &str, body: &str) -> Result<ClientResponse, String> {
        let (host, port, scheme_host) = parse_url(&self.base_url)?;
        let addr = format!("{}:{}", host, port);

        let stream =
            TcpStream::connect(&addr).map_err(|e| format!("cannot connect to {}: {}", addr, e))?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(30)))
            .map_err(|e| format!("set timeout: {}", e))?;

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nUser-Agent: trident/0.1\r\n\r\n{}",
            path, scheme_host, body.len(), body,
        );

        (&stream)
            .write_all(request.as_bytes())
            .map_err(|e| format!("write request: {}", e))?;

        read_response(&stream)
    }
}

struct ClientResponse {
    status: u16,
    body: String,
}

fn read_response(stream: &TcpStream) -> Result<ClientResponse, String> {
    let mut reader = BufReader::new(stream);

    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("read status: {}", e))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(500);

    let mut content_length: usize = 0;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("read header: {}", e))?;
        let line = line.trim().to_string();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim();
            if key == "content-length" {
                content_length = value.parse().unwrap_or(0);
            } else if key == "transfer-encoding" && value.to_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }

    let body = if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        std::io::Read::read_exact(&mut reader, &mut buf)
            .map_err(|e| format!("read body: {}", e))?;
        String::from_utf8(buf).unwrap_or_default()
    } else if chunked {
        let mut body = String::new();
        loop {
            let mut chunk_line = String::new();
            reader
                .read_line(&mut chunk_line)
                .map_err(|e| format!("read chunk size: {}", e))?;
            let chunk_size = usize::from_str_radix(chunk_line.trim(), 16).unwrap_or(0);
            if chunk_size == 0 {
                break;
            }
            let mut chunk = vec![0u8; chunk_size];
            std::io::Read::read_exact(&mut reader, &mut chunk)
                .map_err(|e| format!("read chunk: {}", e))?;
            body.push_str(&String::from_utf8(chunk).unwrap_or_default());
            let mut crlf = String::new();
            let _ = reader.read_line(&mut crlf);
        }
        body
    } else {
        let mut body = String::new();
        let _ = std::io::Read::read_to_string(&mut reader, &mut body);
        body
    };

    Ok(ClientResponse { status, body })
}

// ─── Publish from Local UCM ───────────────────────────────────────

/// Publish all definitions from the local UCM codebase to a registry.
pub fn publish_codebase(
    codebase: &Codebase,
    client: &RegistryClient,
    tags: &[String],
) -> Result<Vec<PublishResult>, String> {
    let names = codebase.list_names();
    let mut results = Vec::new();

    for (name, hash) in &names {
        let def = match codebase.lookup_hash(hash) {
            Some(d) => d,
            None => continue,
        };

        let pub_def = PublishedDefinition {
            hash: hash.to_hex(),
            source: def.source.clone(),
            module: def.module.clone(),
            is_pub: def.is_pub,
            params: def.params.clone(),
            return_ty: def.return_ty.clone(),
            dependencies: def.dependencies.iter().map(|h| h.to_hex()).collect(),
            requires: def.requires.clone(),
            ensures: def.ensures.clone(),
            name: Some(name.to_string()),
            tags: tags.to_vec(),
            verified: false,
            verification_cert: None,
        };

        match client.publish(&pub_def) {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("  warning: failed to publish '{}': {}", name, e);
            }
        }
    }

    Ok(results)
}

/// Pull a definition from a registry into the local UCM codebase.
pub fn pull_into_codebase(
    codebase: &mut Codebase,
    client: &RegistryClient,
    name_or_hash: &str,
) -> Result<PullResult, String> {
    let pull = if name_or_hash.len() == 64 && name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        client.pull(name_or_hash)?
    } else {
        client.pull_by_name(name_or_hash)?
    };

    let hash =
        parse_hex_hash(&pull.hash).ok_or_else(|| "invalid hash in pull response".to_string())?;

    if codebase.lookup_hash(&hash).is_some() {
        return Ok(pull);
    }

    let deps: Vec<ContentHash> = pull
        .dependencies
        .iter()
        .filter_map(|h| parse_hex_hash(h))
        .collect();

    let def = Definition {
        source: pull.source.clone(),
        module: pull.module.clone(),
        is_pub: true,
        params: pull.params.clone(),
        return_ty: pull.return_ty.clone(),
        dependencies: deps,
        requires: pull.requires.clone(),
        ensures: pull.ensures.clone(),
        first_seen: unix_timestamp(),
    };

    codebase.store_definition(hash, def);

    if name_or_hash.len() != 64 || !name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        codebase.bind_name(name_or_hash, hash);
    }

    codebase.save().map_err(|e| e.to_string())?;

    Ok(pull)
}

// ─── JSON Helpers ─────────────────────────────────────────────────

fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_escape_inner(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Find a top-level JSON key (depth 1) and return its byte offset.
/// Skips keys nested inside arrays or sub-objects by tracking
/// brace/bracket nesting while being aware of JSON strings.
fn find_toplevel_key(json: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{}\":", key);
    let bytes = json.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            // Skip over the entire JSON string (key or value).
            // Record the start position — we may need to match here.
            let start = i;
            i += 1; // skip opening quote
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2; // skip escaped char
                } else if bytes[i] == b'"' {
                    i += 1; // skip closing quote
                    break;
                } else {
                    i += 1;
                }
            }
            // At depth 1, check if this position is our needle.
            if depth == 1 && json[start..].starts_with(&needle) {
                return Some(start);
            }
            continue;
        }
        match b {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    None
}

fn extract_json_string(json: &str, key: &str) -> String {
    let needle = format!("\"{}\":", key);
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('"') {
            let inner = &after[1..];
            let mut result = String::new();
            let mut chars = inner.chars();
            while let Some(ch) = chars.next() {
                if ch == '"' {
                    break;
                }
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('r') => result.push('\r'),
                        Some('t') => result.push('\t'),
                        Some('"') => result.push('"'),
                        Some('\\') => result.push('\\'),
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => break,
                    }
                } else {
                    result.push(ch);
                }
            }
            return result;
        }
    }
    String::new()
}

fn extract_json_bool(json: &str, key: &str) -> bool {
    let needle = format!("\"{}\":", key);
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        return after.starts_with("true");
    }
    false
}

fn extract_json_array_strings(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{}\":", key);
    let mut results = Vec::new();
    if let Some(pos) = find_toplevel_key(json, key) {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('[') {
            let bracket_end = find_matching_bracket(after);
            let inner = &after[1..bracket_end];
            for item in inner.split(',') {
                let item = item.trim();
                if item.starts_with('"') && item.ends_with('"') {
                    results.push(item[1..item.len() - 1].to_string());
                }
            }
        }
    }
    results
}

fn find_matching_bracket(s: &str) -> usize {
    let mut depth = 0;
    for (i, ch) in s.chars().enumerate() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len()
}

fn format_definition_json(
    hash: &str,
    name: &str,
    def: &Definition,
    meta: Option<&RegistryMeta>,
) -> String {
    let deps: Vec<String> = def
        .dependencies
        .iter()
        .map(|h| format!("\"{}\"", h.to_hex()))
        .collect();
    let params: Vec<String> = def
        .params
        .iter()
        .map(|(n, t)| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                json_escape(n),
                json_escape(t)
            )
        })
        .collect();
    let requires: Vec<String> = def.requires.iter().map(|r| json_escape(r)).collect();
    let ensures: Vec<String> = def.ensures.iter().map(|e| json_escape(e)).collect();

    let mut json = format!(
        "{{\"hash\":\"{}\",\"name\":{},\"source\":{},\"module\":{},\"is_pub\":{},\"params\":[{}],\"return_ty\":{},\"dependencies\":[{}],\"requires\":[{}],\"ensures\":[{}],\"first_seen\":{}",
        hash,
        json_escape(name),
        json_escape(&def.source),
        json_escape(&def.module),
        def.is_pub,
        params.join(","),
        def.return_ty.as_ref().map(|t| json_escape(t)).unwrap_or_else(|| "null".to_string()),
        deps.join(","),
        requires.join(","),
        ensures.join(","),
        def.first_seen,
    );

    if let Some(m) = meta {
        let tags: Vec<String> = m.tags.iter().map(|t| json_escape(t)).collect();
        json.push_str(&format!(
            ",\"verified\":{},\"tags\":[{}],\"downloads\":{},\"published_at\":{}",
            m.verified,
            tags.join(","),
            m.downloads,
            m.published_at,
        ));
        if let Some(ref cert) = m.verification_cert {
            json.push_str(&format!(",\"verification_cert\":{}", json_escape(cert)));
        }
        if let Some(ref pub_by) = m.publisher {
            json.push_str(&format!(",\"publisher\":{}", json_escape(pub_by)));
        }
    }

    json.push('}');
    json
}

fn format_publish_json(def: &PublishedDefinition) -> String {
    let deps: Vec<String> = def
        .dependencies
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect();
    let params: Vec<String> = def
        .params
        .iter()
        .map(|(n, t)| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                json_escape(n),
                json_escape(t)
            )
        })
        .collect();
    let requires: Vec<String> = def.requires.iter().map(|r| json_escape(r)).collect();
    let ensures: Vec<String> = def.ensures.iter().map(|e| json_escape(e)).collect();
    let tags: Vec<String> = def.tags.iter().map(|t| json_escape(t)).collect();

    format!(
        "{{\"hash\":\"{}\",\"source\":{},\"module\":{},\"is_pub\":{},\"params\":[{}],\"return_ty\":{},\"dependencies\":[{}],\"requires\":[{}],\"ensures\":[{}],\"name\":{},\"tags\":[{}],\"verified\":{},\"verification_cert\":{}}}",
        def.hash,
        json_escape(&def.source),
        json_escape(&def.module),
        def.is_pub,
        params.join(","),
        def.return_ty.as_ref().map(|t| json_escape(t)).unwrap_or_else(|| "null".to_string()),
        deps.join(","),
        requires.join(","),
        ensures.join(","),
        def.name.as_ref().map(|n| json_escape(n)).unwrap_or_else(|| "null".to_string()),
        tags.join(","),
        def.verified,
        def.verification_cert.as_ref().map(|c| json_escape(c)).unwrap_or_else(|| "null".to_string()),
    )
}

fn parse_publish_body(body: &str) -> Result<PublishedDefinition, String> {
    let hash = extract_json_string(body, "hash");
    if hash.is_empty() {
        return Err("missing 'hash' field".to_string());
    }
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid hash format (expected 64 hex chars)".to_string());
    }

    let source = extract_json_string(body, "source");
    if source.is_empty() {
        return Err("missing 'source' field".to_string());
    }

    let module = extract_json_string(body, "module");
    let is_pub = extract_json_bool(body, "is_pub");
    let return_ty = {
        let rt = extract_json_string(body, "return_ty");
        if rt.is_empty() {
            None
        } else {
            Some(rt)
        }
    };

    let params = extract_params_array(body);
    let dependencies = extract_json_array_strings(body, "dependencies");
    let requires = extract_json_array_strings(body, "requires");
    let ensures = extract_json_array_strings(body, "ensures");
    let tags = extract_json_array_strings(body, "tags");
    let name = {
        let n = extract_json_string(body, "name");
        if n.is_empty() {
            None
        } else {
            Some(n)
        }
    };
    let verified = extract_json_bool(body, "verified");
    let verification_cert = {
        let vc = extract_json_string(body, "verification_cert");
        if vc.is_empty() {
            None
        } else {
            Some(vc)
        }
    };

    Ok(PublishedDefinition {
        hash,
        source,
        module,
        is_pub,
        params,
        return_ty,
        dependencies,
        requires,
        ensures,
        name,
        tags,
        verified,
        verification_cert,
    })
}

fn extract_params_array(json: &str) -> Vec<(String, String)> {
    let needle = "\"params\":";
    let mut results = Vec::new();
    if let Some(pos) = find_toplevel_key(json, "params") {
        let after = &json[pos + needle.len()..];
        let after = after.trim_start();
        if after.starts_with('[') {
            let bracket_end = find_matching_bracket(after);
            let inner = &after[1..bracket_end];
            for obj in inner.split("},") {
                let name = extract_json_string(obj, "name");
                let ty = extract_json_string(obj, "type");
                if !name.is_empty() {
                    results.push((name, ty));
                }
            }
        }
    }
    results
}

fn parse_pull_response(body: &str) -> PullResult {
    PullResult {
        hash: extract_json_string(body, "hash"),
        source: extract_json_string(body, "source"),
        module: extract_json_string(body, "module"),
        params: extract_params_array(body),
        return_ty: {
            let rt = extract_json_string(body, "return_ty");
            if rt.is_empty() {
                None
            } else {
                Some(rt)
            }
        },
        dependencies: extract_json_array_strings(body, "dependencies"),
        requires: extract_json_array_strings(body, "requires"),
        ensures: extract_json_array_strings(body, "ensures"),
    }
}

fn parse_search_response(body: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let needle = "\"results\":[";
    if let Some(pos) = body.find(needle) {
        let after = &body[pos + needle.len() - 1..]; // include the [
        let bracket_end = find_matching_bracket(after);
        let inner = &after[1..bracket_end];

        for obj in inner.split("},{") {
            let name = extract_json_string(obj, "name");
            let hash = extract_json_string(obj, "hash");
            let module = extract_json_string(obj, "module");
            let signature = extract_json_string(obj, "signature");
            let verified = extract_json_bool(obj, "verified");
            let tags = extract_json_array_strings(obj, "tags");

            if !hash.is_empty() {
                results.push(SearchResult {
                    name,
                    hash,
                    module,
                    signature,
                    verified,
                    tags,
                });
            }
        }
    }
    results
}

// ─── URL / HTTP Helpers ───────────────────────────────────────────

fn parse_url(url: &str) -> Result<(String, u16, String), String> {
    let url = url.trim();
    let without_scheme = if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else if url.starts_with("https://") {
        return Err("HTTPS not supported (use HTTP for local registries)".to_string());
    } else {
        url
    };

    let (host_port, _path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        let port: u16 = p.parse().map_err(|_| "invalid port".to_string())?;
        (h.to_string(), port)
    } else {
        (host_port.to_string(), 80)
    };

    Ok((host, port, host_port.to_string()))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if query.is_empty() {
        return map;
    }
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(key.to_string(), url_decode(value));
        }
    }
    map
}

fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let hi = chars.next().unwrap_or('0');
            let lo = chars.next().unwrap_or('0');
            let byte = u8::from_str_radix(&format!("{}{}", hi, lo), 16).unwrap_or(b'?');
            out.push(byte as char);
        } else if ch == '+' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn send_response(
    stream: &TcpStream,
    status: u16,
    status_text: &str,
    body: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        status, status_text, body.len(), body,
    );
    (&*stream).write_all(response.as_bytes())
}

fn json_response(status: u16, status_text: &str, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        status_text: status_text.to_string(),
        body: body.to_string(),
    }
}

// ─── Type Signature Formatting ────────────────────────────────────

fn format_type_signature(def: &Definition) -> String {
    let params: Vec<String> = def
        .params
        .iter()
        .map(|(n, t)| format!("{}: {}", n, t))
        .collect();
    let ret = def.return_ty.as_deref().unwrap_or("()");
    format!("fn({}) -> {}", params.join(", "), ret)
}

// ─── Validation ───────────────────────────────────────────────────

fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 256 {
        return false;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

// ─── Hex Hash Parsing ────────────────────────────────────────────

fn parse_hex_hash(hex: &str) -> Option<ContentHash> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 || chunk.len() < 2 {
            return None;
        }
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes[i] = (hi << 4) | lo;
    }
    Some(ContentHash(bytes))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_escape() {
        assert_eq!(json_escape("hello"), "\"hello\"");
        assert_eq!(json_escape("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_escape("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_escape("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(json_escape("tab\there"), "\"tab\\there\"");
    }

    #[test]
    fn test_extract_json_string() {
        let json = r#"{"hash":"abc123","name":"test"}"#;
        assert_eq!(extract_json_string(json, "hash"), "abc123");
        assert_eq!(extract_json_string(json, "name"), "test");
        assert_eq!(extract_json_string(json, "missing"), "");
    }

    #[test]
    fn test_extract_json_bool() {
        let json = r#"{"verified":true,"created":false}"#;
        assert!(extract_json_bool(json, "verified"));
        assert!(!extract_json_bool(json, "created"));
        assert!(!extract_json_bool(json, "missing"));
    }

    #[test]
    fn test_extract_json_array_strings() {
        let json = r#"{"tags":["crypto","hash","verified"]}"#;
        let tags = extract_json_array_strings(json, "tags");
        assert_eq!(tags, vec!["crypto", "hash", "verified"]);
    }

    #[test]
    fn test_extract_json_array_strings_empty() {
        let json = r#"{"tags":[]}"#;
        let tags = extract_json_array_strings(json, "tags");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(url_encode("Field"), "Field");
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("a%2Bb%3Dc"), "a+b=c");
        assert_eq!(url_decode("Field"), "Field");
    }

    #[test]
    fn test_parse_query() {
        let q = parse_query("q=hello&type=Field&limit=10");
        assert_eq!(q.get("q").unwrap(), "hello");
        assert_eq!(q.get("type").unwrap(), "Field");
        assert_eq!(q.get("limit").unwrap(), "10");
    }

    #[test]
    fn test_parse_query_empty() {
        let q = parse_query("");
        assert!(q.is_empty());
    }

    #[test]
    fn test_is_valid_name() {
        assert!(is_valid_name("add"));
        assert!(is_valid_name("std.hash.hash_pair"));
        assert!(is_valid_name("_private"));
        assert!(is_valid_name("CamelCase"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("123abc"));
        assert!(!is_valid_name("a/b"));
        assert!(!is_valid_name("a b"));
    }

    #[test]
    fn test_parse_hex_hash_valid() {
        let hex = "a".repeat(64);
        assert!(parse_hex_hash(&hex).is_some());
    }

    #[test]
    fn test_parse_hex_hash_invalid_length() {
        assert!(parse_hex_hash("abc").is_none());
        assert!(parse_hex_hash(&"a".repeat(63)).is_none());
        assert!(parse_hex_hash(&"a".repeat(65)).is_none());
    }

    #[test]
    fn test_parse_hex_hash_invalid_chars() {
        let mut hex = "a".repeat(64);
        hex.replace_range(0..1, "g");
        assert!(parse_hex_hash(&hex).is_none());
    }

    #[test]
    fn test_parse_url() {
        let (host, port, _) = parse_url("http://127.0.0.1:8090").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8090);

        let (host, port, _) = parse_url("http://localhost").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 80);
    }

    #[test]
    fn test_registry_meta_roundtrip() {
        let mut meta = RegistryMeta::new();
        meta.tags = vec!["crypto".to_string(), "hash".to_string()];
        meta.verified = true;
        meta.downloads = 42;
        meta.publisher = Some("alice".to_string());
        meta.verification_cert = Some("cert_data_here".to_string());

        let serialized = meta.serialize();
        let deserialized = RegistryMeta::deserialize(&serialized);

        assert_eq!(deserialized.tags, meta.tags);
        assert_eq!(deserialized.verified, meta.verified);
        assert_eq!(deserialized.downloads, meta.downloads);
        assert_eq!(deserialized.publisher, meta.publisher);
        assert_eq!(deserialized.verification_cert, meta.verification_cert);
    }

    #[test]
    fn test_format_type_signature() {
        let def = Definition {
            source: String::new(),
            module: "test".to_string(),
            is_pub: true,
            params: vec![
                ("a".to_string(), "Field".to_string()),
                ("b".to_string(), "Field".to_string()),
            ],
            return_ty: Some("Field".to_string()),
            dependencies: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
            first_seen: 0,
        };
        let sig = format_type_signature(&def);
        assert_eq!(sig, "fn(a: Field, b: Field) -> Field");
    }

    #[test]
    fn test_format_type_signature_void() {
        let def = Definition {
            source: String::new(),
            module: "test".to_string(),
            is_pub: false,
            params: Vec::new(),
            return_ty: None,
            dependencies: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
            first_seen: 0,
        };
        let sig = format_type_signature(&def);
        assert_eq!(sig, "fn() -> ()");
    }

    #[test]
    fn test_format_definition_json() {
        let def = Definition {
            source: "fn add(a: Field, b: Field) -> Field { a + b }".to_string(),
            module: "math".to_string(),
            is_pub: true,
            params: vec![
                ("a".to_string(), "Field".to_string()),
                ("b".to_string(), "Field".to_string()),
            ],
            return_ty: Some("Field".to_string()),
            dependencies: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
            first_seen: 1000,
        };
        let hash = "a".repeat(64);
        let json = format_definition_json(&hash, "add", &def, None);

        assert!(json.contains(&hash));
        assert!(json.contains("\"name\":\"add\""));
        assert!(json.contains("\"module\":\"math\""));
        assert!(json.contains("\"is_pub\":true"));
        assert!(json.contains("\"first_seen\":1000"));
    }

    #[test]
    fn test_format_definition_json_with_meta() {
        let def = Definition {
            source: "fn id(x: Field) -> Field { x }".to_string(),
            module: "core".to_string(),
            is_pub: true,
            params: vec![("x".to_string(), "Field".to_string())],
            return_ty: Some("Field".to_string()),
            dependencies: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
            first_seen: 2000,
        };
        let meta = RegistryMeta {
            tags: vec!["core".to_string()],
            verified: true,
            verification_cert: None,
            downloads: 100,
            published_at: 2000,
            publisher: None,
        };
        let hash = "b".repeat(64);
        let json = format_definition_json(&hash, "id", &def, Some(&meta));

        assert!(json.contains("\"verified\":true"));
        assert!(json.contains("\"downloads\":100"));
        assert!(json.contains("\"tags\":[\"core\"]"));
    }

    #[test]
    fn test_publish_json_roundtrip() {
        let pub_def = PublishedDefinition {
            hash: "c".repeat(64),
            source: "fn test() { }".to_string(),
            module: "test_mod".to_string(),
            is_pub: false,
            params: Vec::new(),
            return_ty: None,
            dependencies: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
            name: Some("test_fn".to_string()),
            tags: vec!["testing".to_string()],
            verified: false,
            verification_cert: None,
        };

        let json = format_publish_json(&pub_def);
        let parsed = parse_publish_body(&json).unwrap();

        assert_eq!(parsed.hash, pub_def.hash);
        assert_eq!(parsed.source, pub_def.source);
        assert_eq!(parsed.module, pub_def.module);
        assert_eq!(parsed.is_pub, pub_def.is_pub);
        assert_eq!(parsed.name, pub_def.name);
        assert_eq!(parsed.tags, pub_def.tags);
    }

    #[test]
    fn test_publish_json_roundtrip_complex() {
        let pub_def = PublishedDefinition {
            hash: "d".repeat(64),
            source: "fn add(a: Field, b: Field) -> Field {\n    a + b\n}".to_string(),
            module: "std.math".to_string(),
            is_pub: true,
            params: vec![
                ("a".to_string(), "Field".to_string()),
                ("b".to_string(), "Field".to_string()),
            ],
            return_ty: Some("Field".to_string()),
            dependencies: vec!["e".repeat(64)],
            requires: vec!["a > 0".to_string()],
            ensures: vec!["result == a + b".to_string()],
            name: Some("add".to_string()),
            tags: vec!["math".to_string(), "core".to_string()],
            verified: true,
            verification_cert: Some("cert123".to_string()),
        };

        let json = format_publish_json(&pub_def);
        let parsed = parse_publish_body(&json).unwrap();

        assert_eq!(parsed.hash, pub_def.hash);
        assert_eq!(parsed.source, pub_def.source);
        assert_eq!(parsed.module, pub_def.module);
        assert_eq!(parsed.is_pub, pub_def.is_pub);
        assert_eq!(parsed.params, pub_def.params);
        assert_eq!(parsed.return_ty, pub_def.return_ty);
        assert_eq!(parsed.name, pub_def.name);
        assert_eq!(parsed.verified, pub_def.verified);
    }

    #[test]
    fn test_parse_publish_body_missing_hash() {
        let body = r#"{"source":"fn test() { }"}"#;
        assert!(parse_publish_body(body).is_err());
    }

    #[test]
    fn test_parse_publish_body_missing_source() {
        let hash = "a".repeat(64);
        let body = format!("{{\"hash\":\"{}\"}}", hash);
        assert!(parse_publish_body(&body).is_err());
    }

    #[test]
    fn test_parse_publish_body_invalid_hash() {
        let body = r#"{"hash":"tooshort","source":"fn test() { }"}"#;
        assert!(parse_publish_body(body).is_err());
    }

    #[test]
    fn test_route_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let query = HashMap::new();

        let resp = route_request("GET", "/api/v1/../../etc/passwd", &query, "", &state);
        assert_eq!(resp.status, 400);
    }

    #[test]
    fn test_route_health() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let query = HashMap::new();

        let resp = route_request("GET", "/health", &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"status\":\"ok\""));
    }

    #[test]
    fn test_route_stats_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let query = HashMap::new();

        let resp = route_request("GET", "/api/v1/stats", &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"definitions\":0"));
    }

    #[test]
    fn test_route_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let query = HashMap::new();

        let resp = route_request("GET", "/nonexistent", &query, "", &state);
        assert_eq!(resp.status, 404);
    }

    #[test]
    fn test_route_search_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let mut query = HashMap::new();
        query.insert("q".to_string(), "test".to_string());

        let resp = route_request("GET", "/api/v1/search", &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"count\":0"));
    }

    #[test]
    fn test_route_names_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));
        let query = HashMap::new();

        let resp = route_request("GET", "/api/v1/names", &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"names\":[]"));
    }

    #[test]
    fn test_route_publish_and_retrieve() {
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(Mutex::new(RegistryState::load(tmp.path()).unwrap()));

        // Publish a definition.
        let hash = "a".repeat(64);
        let body = format!(
            "{{\"hash\":\"{}\",\"source\":\"fn add(a: Field, b: Field) -> Field {{ a + b }}\",\"module\":\"math\",\"is_pub\":true,\"params\":[{{\"name\":\"a\",\"type\":\"Field\"}},{{\"name\":\"b\",\"type\":\"Field\"}}],\"return_ty\":\"Field\",\"dependencies\":[],\"requires\":[],\"ensures\":[],\"name\":\"add\",\"tags\":[\"math\"],\"verified\":false,\"verification_cert\":null}}",
            hash
        );
        let query = HashMap::new();

        let resp = route_request("POST", "/api/v1/definitions", &query, &body, &state);
        assert_eq!(resp.status, 201);
        assert!(resp.body.contains("\"created\":true"));

        // Retrieve by hash.
        let path = format!("/api/v1/definitions/{}", hash);
        let resp = route_request("GET", &path, &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"name\":\"add\""));

        // Resolve by name.
        let resp = route_request("GET", "/api/v1/names/add", &query, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains(&hash));

        // Search by name.
        let mut sq = HashMap::new();
        sq.insert("q".to_string(), "add".to_string());
        let resp = route_request("GET", "/api/v1/search", &sq, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"count\":1"));

        // Search by type.
        let mut tq = HashMap::new();
        tq.insert("type".to_string(), "field".to_string());
        let resp = route_request("GET", "/api/v1/search", &tq, "", &state);
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("\"count\":1"));
    }
}
