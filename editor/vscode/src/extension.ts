import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const serverOptions: ServerOptions = {
    command: "trident-lsp",
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "trident" }],
  };

  client = new LanguageClient(
    "trident-lsp",
    "Trident LSP",
    serverOptions,
    clientOptions,
  );

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
