use std::{collections::HashMap, sync::Mutex, time::Duration};

use actix_web::{
    error::ErrorInternalServerError,
    get,
    web::{self, Redirect},
};
use anyhow::Context;
use reqwest::StatusCode;

use crate::CLIENT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerFileStatus {
    Exists,
    NotFound,
    PeerError,
}

async fn check_file_exists(peer: &str, port: u16, file_name: &str) -> PeerFileStatus {
    let url = format!("http://{}:{}/cache/{}", peer, port, file_name);
    let response = CLIENT
        .head(&url)
        .timeout(Duration::from_secs(1))
        .send()
        .await;
    match response {
        Ok(resp) => {
            if resp.status() == StatusCode::NOT_FOUND {
                PeerFileStatus::NotFound
            } else if resp.status().is_success() {
                PeerFileStatus::Exists
            } else {
                PeerFileStatus::PeerError
            }
        }
        Err(_) => PeerFileStatus::PeerError,
    }
}

#[get("/{arch}/{repo}/{file_name}")]
async fn service_proxy(
    path: web::Path<(String, String, String)>,
    peer_list: web::Data<Mutex<HashMap<String, u16>>>,
    upstream_urls: web::Data<HashMap<String, Vec<String>>>,
) -> Result<Redirect, actix_web::Error> {
    let (arch, repo, file_name) = path.as_ref();
    if file_name.ends_with(".db")
        || file_name.ends_with(".files")
        || file_name.ends_with(".db.sig")
        || file_name.ends_with(".files.sig")
    {
        return redirect_to_upstream(repo, arch, file_name, &upstream_urls).await;
    }
    let cloned_peer_list = peer_list.lock().unwrap().clone();
    for (peer, port) in cloned_peer_list.into_iter() {
        if !file_name.ends_with(".sig") {
            let status = check_file_exists(&peer, port, &format!("{file_name}.sig")).await;
            match status {
                PeerFileStatus::Exists => {}
                PeerFileStatus::NotFound => {
                    continue;
                }
                PeerFileStatus::PeerError => {
                    peer_list.lock().unwrap().remove(&peer);
                    continue;
                }
            }
        }
        let status = check_file_exists(&peer, port, file_name).await;
        match status {
            PeerFileStatus::Exists => {
                let url = format!("http://{}:{}/cache/{}", peer, port, file_name);
                return Ok(Redirect::to(url).temporary());
            }
            PeerFileStatus::NotFound => {}
            PeerFileStatus::PeerError => {
                peer_list.lock().unwrap().remove(&peer);
            }
        }
    }
    redirect_to_upstream(repo, arch, file_name, &upstream_urls).await
}
async fn redirect_to_upstream(
    repo: &str,
    arch: &str,
    file_name: &str,
    upstream_urls: &HashMap<String, Vec<String>>,
) -> Result<Redirect, actix_web::Error> {
    let upstream_urls = upstream_urls
        .get(repo)
        .context(format!("Repository {} is not available", repo))
        .map_err(ErrorInternalServerError)?;
    let upstream_url = upstream_urls
        .get(0)
        .context(format!(
            "Repository {} is available but no upstream found",
            repo
        ))
        .map_err(ErrorInternalServerError)?;
    let real_url = upstream_url.replace("$repo", &repo).replace("$arch", &arch) + "/" + &file_name;
    Ok(Redirect::to(real_url).temporary())
}
