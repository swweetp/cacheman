use std::{collections::HashMap, path::Path};

use anyhow::Result;

use super::pacman_conf;

async fn get_repositories(config_file_path: Option<&Path>) -> Result<Vec<String>> {
    let output = pacman_conf(config_file_path, ["--repo-list"]).await?;
    let repositories = output
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    Ok(repositories)
}

async fn get_urls_from_repository(
    config_file_path: Option<&Path>,
    repository: &str,
) -> Result<Vec<String>> {
    let output = pacman_conf(config_file_path, ["--repo", repository, "Server"]).await?;
    let mut urls = Vec::new();
    for line in output.lines() {
        urls.push(line.to_string());
    }
    Ok(urls)
}
pub async fn get_all_repository_urls(
    config_file_path: Option<&Path>,
) -> Result<HashMap<String, Vec<String>>> {
    let mut all_urls = HashMap::new();
    for repo in get_repositories(config_file_path).await? {
        let urls = get_urls_from_repository(config_file_path, &repo).await?;
        all_urls.insert(repo, urls);
    }
    Ok(all_urls)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use anyhow::Result;
    use indoc::indoc;

    use crate::{
        get_pacman_configuration::upstream_url::{
            get_all_repository_urls, get_repositories, get_urls_from_repository,
        },
        test_utils::generate_config_file,
    };

    #[tokio::test]
    async fn test_get_repositories() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            Architecture = auto
            [core]
            Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
            "
        ))
        .await?;
        let repositories = get_repositories(Some(&config_file_path)).await?;
        assert_eq!(repositories, vec!["core"]);
        Ok(())
    }
    #[tokio::test]
    async fn test_get_urls_from_repository() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            Architecture = auto
            [core]
            Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
            "
        ))
        .await?;
        let urls = get_urls_from_repository(Some(&config_file_path), "core").await?;
        assert_eq!(urls, vec!["https://geo.mirror.pkgbuild.com/core/os/x86_64"]);
        Ok(())
    }
    #[tokio::test]
    async fn test_get_urls_from_repository_not_found() -> Result<()> {
        let (_d, config_file_path) = generate_config_file(indoc!(
            "
            [options]
            Architecture = auto
            [core]
            Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
            [extra]
            Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
            [multilib]
            Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch
            "
        ))
        .await?;
        let list_1 = [
            ("core", "https://geo.mirror.pkgbuild.com/core/os/x86_64"),
            ("extra", "https://geo.mirror.pkgbuild.com/extra/os/x86_64"),
            (
                "multilib",
                "https://geo.mirror.pkgbuild.com/multilib/os/x86_64",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), vec![v.to_string()]))
        .collect::<HashMap<_, _>>();
        assert_eq!(
            get_all_repository_urls(Some(&config_file_path)).await?,
            list_1
        );

        Ok(())
    }
}
