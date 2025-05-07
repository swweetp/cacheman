use std::{
    collections::HashMap,
    net::Ipv4Addr,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use actix_files::Files;
use actix_web::{
    HttpServer,
    guard::fn_guard,
    web::{Data, scope},
};
use anyhow::{Context, Result, ensure};
use get_pacman_configuration::{cache_dir::get_cache_dirs, upstream_url::get_all_repository_urls};
use neighbor_discovery::{advertise::Advertiser, browse::Browser};
use reqwest::Client;
use service::service_proxy;
use tokio::spawn;

mod get_pacman_configuration;
mod neighbor_discovery;
mod service;
#[cfg(test)]
pub mod test_utils;

const CLIENT: LazyLock<Client> = LazyLock::new(Client::new);

async fn check_is_valid_upstream(url_base: &str, repository: &str) -> bool {
    let mut db_file_url = url_base.to_string();
    if !db_file_url.ends_with("/") {
        db_file_url.push('/');
    }
    db_file_url.push_str(repository);
    db_file_url.push_str(".db");

    let Ok(result) = CLIENT
        .head(&db_file_url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
    else {
        return false;
    };
    result.status().is_success()
}

#[tokio::main]
async fn main() -> Result<()> {
    const PORT: u16 = 1052;
    let pacman_cache_dirs = get_cache_dirs(None).await?;
    ensure!(
        !pacman_cache_dirs.is_empty(),
        "No cache directories found in pacman configuration"
    );
    // TODO: 複数キャッシュディレクトリに対応
    let pacman_cache_dir = pacman_cache_dirs[0].clone();
    // TODO: 複数リポジトリに対応
    let mut upstream_urls = get_all_repository_urls(None)
        .await
        .context("Failed to get upstream URLs")?;

    for (repository, urls) in upstream_urls.iter_mut() {
        let mut handles = Vec::new();
        for url in urls.iter() {
            let h = spawn({
                let url = url.clone();
                let repository = repository.clone();
                async move { check_is_valid_upstream(&url, &repository).await }
            });
            handles.push((h, url.clone()));
        }
        let mut new_urls = Vec::new();
        for (handle, url) in handles {
            let result = handle.await?;
            if result {
                new_urls.push(url);
            }
        }

        *urls = new_urls;
    }
    let upstream_urls = Data::new(upstream_urls);

    let _advertiser = Advertiser::new(
        hostname::get()?
            .to_str()
            .context("Failed to get hostname")?,
        PORT,
    )
    .await?;

    let mut peer_list = HashMap::new();
    for peer in Browser::new().await?.get_updated_items().await? {
        peer_list.insert(peer.hostname, PORT);
    }
    let peer_list = Data::new(Mutex::new(peer_list));

    HttpServer::new(move || {
        actix_web::App::new()
            .service(
                scope("/cache").service(
                    Files::new("/", &pacman_cache_dir)
                        .show_files_listing()
                        .use_last_modified(true),
                ),
            )
            .service(
                scope("/proxy")
                    .guard(fn_guard(|ctx| {
                        let Some(addr) = ctx.head().peer_addr else {
                            return false;
                        };
                        addr.ip().is_loopback()
                    }))
                    .app_data(peer_list.clone())
                    .app_data(upstream_urls.clone())
                    .service(service_proxy),
            )
    })
    .bind((Ipv4Addr::UNSPECIFIED, PORT))?
    .run()
    .await?;
    Ok(())
}
