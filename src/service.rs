mod redirect_upstream;

use std::{collections::HashMap, sync::Mutex, time::Duration};

use actix_web::{
    error::ErrorInternalServerError,
    get,
    web::{self, Redirect},
};
use anyhow::Result;
use redirect_upstream::redirect_to_upstream;
use reqwest::StatusCode;
use tokio::task::JoinSet;

use crate::CLIENT;

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
    async fn generate_response(&self, arch: &str, repo: &str, file_name: &str) -> Result<String> {
        if is_cachable_file(file_name) {
            if let Some(url) = self.find_peer_with_cache(file_name).await {
                return Ok(url);
            }
        }
        redirect_to_upstream(repo, arch, file_name, &self.upstream_urls).await
    }

    fn mark_peer_unavailable(&self, peer: &str) {
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
                Err(e) => self.mark_peer_unavailable(&e),
            };
        }
        None
    }
}

#[get("/{arch}/{repo}/{file_name}")]
async fn service_proxy(
    path: web::Path<(String, String, String)>,
    proxy_service: web::Data<ProxyService>,
) -> Result<Redirect, actix_web::Error> {
    let (arch, repo, file_name) = path.into_inner();
    match proxy_service
        .generate_response(&arch, &repo, &file_name)
        .await
    {
        Ok(url) => Ok(Redirect::to(url).temporary()),
        Err(e) => Err(ErrorInternalServerError(e)),
    }
}
fn is_cachable_file(file_name: &str) -> bool {
    !file_name.ends_with(".db")
        && !file_name.ends_with(".files")
        && !file_name.ends_with(".db.sig")
        && !file_name.ends_with(".files.sig")
}

async fn test_peer_with_filename(peer: &str, port: u16, file_name: &str) -> Result<bool> {
    let url = format!("http://{}:{}/cache/{}", peer, port, file_name);
    if !file_name.ends_with(".sig") && !check_is_url_found(&format!("{}.sig", &url)).await? {
        return Ok(false);
    }
    check_is_url_found(&url).await
}
async fn check_is_url_found(url: &str) -> Result<bool> {
    let response = CLIENT
        .head(url)
        .timeout(Duration::from_secs(1))
        .send()
        .await;
    match response {
        Ok(resp) => {
            if resp.status() == StatusCode::NOT_FOUND {
                return Ok(false);
            }
            if resp.status().is_success() {
                return Ok(true);
            }
            return Err(anyhow::anyhow!("Unexpected status code: {}", resp.status()));
        }
        Err(e) => return Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, mem::forget, net::Ipv6Addr};

    use actix_web::{
        App,
        http::StatusCode,
        test::{TestRequest, call_service, init_service},
    };
    use anyhow::Result;
    use httpmock::MockServer;

    use tokio::{io::AsyncWriteExt, net::TcpListener, spawn, task::JoinHandle};

    use super::*;

    #[test]
    fn test_is_cachable_file() {
        assert!(is_cachable_file("pkg.tar.zst"));
        assert!(is_cachable_file("pkg.tar.zst.sig"));
        assert!(is_cachable_file("pkg.tar.xz"));
        assert!(is_cachable_file("pkg.tar.xz.sig"));
        assert!(!is_cachable_file("extra.db"));
        assert!(!is_cachable_file("extra.db.sig"));
        assert!(!is_cachable_file("extra.files"));
        assert!(!is_cachable_file("extra.files.sig"));
    }
    #[tokio::test]
    async fn test_find_peer_with_cache() -> Result<()> {
        let mock = MockServer::start();
        mock.mock(|when, then| {
            when.path("/cache/test_file.txt");
            then.status(200);
        });
        mock.mock(|when, then| {
            when.path("/cache/test_file.txt.sig");
            then.status(200);
        });
        mock.mock(|when, then| {
            when.path("/cache/without_sig.txt");
            then.status(200);
        });
        mock.mock(|when, then| {
            when.path("/cache/only_sig.txt.sig");
            then.status(200);
        });

        assert!(test_peer_with_filename(&mock.host(), mock.port(), "test_file.txt").await?);
        assert!(test_peer_with_filename(&mock.host(), mock.port(), "test_file.txt.sig").await?);
        assert!(!test_peer_with_filename(&mock.host(), mock.port(), "without_sig.txt").await?);
        assert!(!test_peer_with_filename(&mock.host(), mock.port(), "only_sig.txt").await?);
        assert!(
            !test_peer_with_filename(&mock.host(), mock.port(), "non_existent_file.txt").await?
        );

        Ok(())
    }
    #[tokio::test]
    async fn test_check_url() -> Result<()> {
        let mock = MockServer::start();
        mock.mock(|when, then| {
            when.path("/cache/test_file.txt");
            then.status(200);
        });
        mock.mock(|when, then| {
            when.path("/cache/uncached_file.txt");
            then.status(404);
        });
        mock.mock(|when, then| {
            when.path("/cache/server_error.txt");
            then.status(500);
        });

        assert!(check_is_url_found(&format!("{}/cache/test_file.txt", mock.base_url())).await?);
        assert!(
            !check_is_url_found(&format!("{}/cache/uncached_file.txt", mock.base_url())).await?
        );
        assert!(
            check_is_url_found(&format!("{}/cache/server_error.txt", mock.base_url()))
                .await
                .is_err()
        );
        {
            let listener = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).await?;
            let url = format!(
                "{}/cache/file_on_broken_server.txt",
                listener.local_addr()?.to_string()
            );
            let _: JoinHandle<Result<Infallible>> = spawn(async move {
                loop {
                    let (mut stream, _) = listener.accept().await?;
                    stream.shutdown().await?;
                }
            });
            assert!(check_is_url_found(&url).await.is_err());
        }
        {
            let listener = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).await?;
            let url = format!(
                "{}/cache/file_on_broken_server.txt",
                listener.local_addr()?.to_string()
            );
            let _: JoinHandle<Result<Infallible>> = spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await?;
                    forget(stream);
                }
            });
            assert!(check_is_url_found(&url).await.is_err());
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_proxy() -> Result<()> {
        let app = init_service(
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
        let req = TestRequest::get()
            .uri("/x86_64/extra/extra.db")
            .to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);

        Ok(())
    }
}
