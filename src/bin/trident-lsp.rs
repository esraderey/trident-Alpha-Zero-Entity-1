use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct TridentLsp {
    client: Client,
}

#[tower_lsp::async_trait]
impl LanguageServer for TridentLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "trident-lsp initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let source = params.text_document.text;
        self.publish_diagnostics(uri, &source).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            self.publish_diagnostics(uri, &change.text).await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

impl TridentLsp {
    async fn publish_diagnostics(&self, uri: Url, source: &str) {
        let filename = uri.path();

        let diagnostics = match trident::check_silent(source, filename) {
            Ok(()) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|d| to_lsp_diagnostic(&d, source))
                .collect(),
        };

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

fn to_lsp_diagnostic(
    diag: &trident::diagnostic::Diagnostic,
    source: &str,
) -> Diagnostic {
    let start = byte_offset_to_position(source, diag.span.start as usize);
    let end = byte_offset_to_position(source, diag.span.end as usize);

    let severity = match diag.severity {
        trident::diagnostic::Severity::Error => DiagnosticSeverity::ERROR,
        trident::diagnostic::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    let mut message = diag.message.clone();
    for note in &diag.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    if let Some(help) = &diag.help {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        source: Some("trident".to_string()),
        message,
        ..Default::default()
    }
}

fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| TridentLsp { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
