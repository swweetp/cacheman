use std::path::Path;

use anyhow::{Result, ensure};
use tokio::process::Command;

pub mod cache_dir;
pub mod upstream_url;

pub async fn pacman_conf(
    config_file_path: Option<&Path>,
    args: impl IntoIterator<Item = &str>,
) -> Result<String> {
    let mut command = Command::new("pacman-conf");
    command.args(args);
    if let Some(config_file_path) = config_file_path {
        command.arg("--config").arg(config_file_path);
    }
    let output = command.output().await?;
    ensure!(
        output.status.success(),
        "pacman-conf failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::get_pacman_configuration::pacman_conf;

    #[tokio::test]
    async fn fail() -> Result<()> {
        let output = pacman_conf(None, ["--invalid-option"]).await;
        assert!(output.is_err(), "Expected error, but got: {:?}", output);
        Ok(())
    }
}
