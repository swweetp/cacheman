use std::path::PathBuf;

use anyhow::Result;
use tempfile::{TempDir, tempdir};
use tokio::fs::write;

pub async fn generate_config_file(content: &str) -> Result<(TempDir, PathBuf)> {
    let config_file_parent_dir = tempdir()?;
    let config_file_path = config_file_parent_dir.path().join("pacman.conf");
    write(&config_file_path, content).await?;
    Ok((config_file_parent_dir, config_file_path))
}
#[macro_export]
macro_rules! location {
    () => {
        &format!("{}-{}-{}", module_path!(), line!(), column!()).replace("::", "-")
    };
}
