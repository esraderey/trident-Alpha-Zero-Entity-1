use zed_extension_api::{self as zed, Result};

struct TridentExtension;

impl zed::Extension for TridentExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let env = worktree.shell_env();
        let home_bin = || {
            env.iter()
                .find(|(k, _)| k == "HOME")
                .map(|(_, home)| format!("{}/.cargo/bin", home))
        };

        if let Some(path) = worktree.which("trident-lsp") {
            return Ok(zed::Command {
                command: path,
                args: vec![],
                env,
            });
        }

        if let Some(bin) = home_bin() {
            let path = format!("{}/trident-lsp", bin);
            return Ok(zed::Command {
                command: path,
                args: vec![],
                env,
            });
        }

        Err("trident-lsp not found. Run: cargo install --path <trident-repo>".into())
    }
}

zed::register_extension!(TridentExtension);
