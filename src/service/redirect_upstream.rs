use std::collections::HashMap;

use anyhow::{Context, Result};

pub async fn redirect_to_upstream(
    repo: &str,
    arch: &str,
    file_name: &str,
    upstream_urls: &HashMap<String, Vec<String>>,
) -> Result<String> {
    let upstream_urls = upstream_urls
        .get(repo)
        .context(format!("Repository {} is not available", repo))?;
    let upstream_url = upstream_urls.get(0).context(format!(
        "Repository {} is available but no upstream found",
        repo
    ))?;
    let real_url = upstream_url.replace("$repo", &repo).replace("$arch", &arch) + "/" + &file_name;
    Ok(real_url)
}
