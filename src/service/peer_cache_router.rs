use std::{collections::HashMap, sync::Mutex};

use anyhow::Result;
use tokio::task::JoinSet;

use super::{is_cachable_file, redirect_upstream::redirect_to_upstream, test_peer_with_filename};

pub struct PeerCacheRouter {
    peer_list: Mutex<HashMap<String, u16>>,
    upstream_urls: HashMap<String, Vec<String>>,
}
impl PeerCacheRouter {
    pub fn new(
        peer_list: Mutex<HashMap<String, u16>>,
        upstream_urls: HashMap<String, Vec<String>>,
    ) -> Self {
        PeerCacheRouter {
            peer_list,
            upstream_urls,
        }
    }
    pub async fn route_redirection(
        &self,
        arch: &str,
        repo: &str,
        file_name: &str,
    ) -> Result<String> {
        if is_cachable_file(file_name) {
            if let Some(url) = self.find_peer_with_cache(file_name).await {
                return Ok(url);
            }
        }
        redirect_to_upstream(repo, arch, file_name, &self.upstream_urls).await
    }

    fn set_peer_unavailable(&self, peer: &str) {
        self.peer_list.lock().unwrap().remove(peer);
    }

    async fn find_peer_with_cache(&self, file_name: &str) -> Option<String> {
        let mut js = JoinSet::new();
        for (peer, &port) in self.peer_list.lock().unwrap().iter() {
            let file_name = file_name.to_string();
            let peer = peer.to_string();
            js.spawn(async move {
                match test_peer_with_filename(&peer, port, &file_name).await {
                    Ok(true) => Ok(Some(peer)),
                    Ok(false) => Ok(None),
                    Err(_) => Err(peer),
                }
            });
        }
        while let Some(res) = js.join_next().await {
            match res.unwrap() {
                Ok(Some(url)) => return Some(url),
                Ok(None) => continue,
                Err(e) => self.set_peer_unavailable(&e),
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App,
        http::StatusCode,
        test::{TestRequest, call_service, init_service},
        web,
    };

    use crate::service::service_proxy;

    use super::*;

    #[tokio::test]
    async fn test_proxy() -> Result<()> {
        let app = init_service(
            App::new()
                .app_data(web::Data::new(PeerCacheRouter::new(
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
        let req = TestRequest::get()
            .uri("/x86_64/extra/extra.db")
            .to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
        let req = TestRequest::get()
            .uri("/x86_64/invalid_repo/invalid.db")
            .to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let req = TestRequest::get()
            .uri("/x86_64/core/hoge.pkg.tar.zst")
            .to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        Ok(())
    }
}
