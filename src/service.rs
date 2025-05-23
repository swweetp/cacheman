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

async fn check_url_exists(peer: &str, port: u16, file_name: &str) -> PeerFileStatus {
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

pub struct ProxyService {
    peer_list: Mutex<HashMap<String, u16>>,
    upstream_urls: HashMap<String, Vec<String>>,
}
impl ProxyService {
    pub fn new(
        peer_list: Mutex<HashMap<String, u16>>,
        upstream_urls: HashMap<String, Vec<String>>,
    ) -> Self {
        ProxyService {
            peer_list,
            upstream_urls,
        }
    }

    async fn generate_response(
        &self,
        arch: &str,
        repo: &str,
        file_name: &str,
    ) -> Result<Redirect, actix_web::Error> {
        if file_name.ends_with(".db")
            || file_name.ends_with(".files")
            || file_name.ends_with(".db.sig")
            || file_name.ends_with(".files.sig")
        {
            return redirect_to_upstream(repo, arch, file_name, &self.upstream_urls).await;
        }
        let cloned_peer_list = self.peer_list.lock().unwrap().clone();
        for (peer, port) in cloned_peer_list.into_iter() {
            if !file_name.ends_with(".sig") {
                let status = check_url_exists(&peer, port, &format!("{file_name}.sig")).await;
                match status {
                    PeerFileStatus::Exists => {}
                    PeerFileStatus::NotFound => {
                        continue;
                    }
                    PeerFileStatus::PeerError => {
                        self.peer_list.lock().unwrap().remove(&peer);
                        continue;
                    }
                }
            }
            let status = check_url_exists(&peer, port, file_name).await;
            match status {
                PeerFileStatus::Exists => {
                    let url = format!("http://{}:{}/cache/{}", peer, port, file_name);
                    return Ok(Redirect::to(url).temporary());
                }
                PeerFileStatus::NotFound => {
                    continue;
                }
                PeerFileStatus::PeerError => {
                    self.peer_list.lock().unwrap().remove(&peer);
                    continue;
                }
            }
        }
        redirect_to_upstream(repo, arch, file_name, &self.upstream_urls).await
    }
}

#[get("/{arch}/{repo}/{file_name}")]
async fn service_proxy(
    path: web::Path<(String, String, String)>,
    proxy_service: web::Data<ProxyService>,
) -> Result<Redirect, actix_web::Error> {
    let (arch, repo, file_name) = path.into_inner();
    proxy_service
        .generate_response(&arch, &repo, &file_name)
        .await
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

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test};
    use anyhow::Result;

    use super::*;

    #[tokio::test]
    async fn test_proxy() -> Result<()> {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(ProxyService::new(
                    Mutex::new(HashMap::new()),
                    [(
                        "extra".to_string(),
                        vec!["http://example.com/x86_64/extra/$repo/$arch/".to_string()],
                    )]
                    .into_iter()
                    .collect(),
                )))
                .service(service_proxy),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/x86_64/extra/extra.db")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        Ok(())
    }
}
