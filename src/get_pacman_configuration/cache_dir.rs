use std::path::{Path, PathBuf};

use anyhow::Result;

use super::pacman_conf;

pub async fn get_cache_dirs(config_file_path: Option<&Path>) -> Result<Vec<PathBuf>> {
    let output = pacman_conf(config_file_path, ["CacheDir"]).await?;
    let cache_dirs = output
        .lines()
        .map(|line| PathBuf::from(line))
        .collect::<Vec<_>>();
    Ok(cache_dirs)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use indoc::indoc;

    use crate::{
        get_pacman_configuration::cache_dir::get_cache_dirs, test_utils::generate_config_file,
    };

    #[tokio::test]
    async fn default_cache_dir() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            "
        ))
        .await?;
        let output = get_cache_dirs(Some(&config_file_path)).await?;
        assert_eq!(output, vec![PathBuf::from("/var/cache/pacman/pkg")]);
        Ok(())
    }
    #[tokio::test]
    async fn custom_cache_dir() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            CacheDir = /tmp/pacman-cache
            "
        ))
        .await?;
        let output = get_cache_dirs(Some(&config_file_path)).await?;
        assert_eq!(output, vec![PathBuf::from("/tmp/pacman-cache")]);
        Ok(())
    }
    #[tokio::test]
    async fn multiple_cache_dirs() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            CacheDir = /var/cache/pacman/pkg
            CacheDir = /tmp/pacman-cache
            "
        ))
        .await?;
        let output = get_cache_dirs(Some(&config_file_path)).await?;
        assert_eq!(
            output,
            vec![
                PathBuf::from("/var/cache/pacman/pkg"),
                PathBuf::from("/tmp/pacman-cache")
            ]
        );
        Ok(())
    }
}
