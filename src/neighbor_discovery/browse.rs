use std::{
    collections::HashSet,
    convert::Infallible,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, ensure};
use futures::StreamExt;
use tokio::{
    spawn,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zbus::Connection;

use super::{
    DESTINATION, SERVICE_TYPE,
    zbus_binding::{server2::Server2Proxy, service_browser::ServiceBrowserProxy},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostInfo {
    pub hostname: String,
}
pub struct Browser {
    current_items: Arc<Mutex<HashSet<HostInfo>>>,
    change_receiver: mpsc::Receiver<()>,
    callback_handles: Vec<JoinHandle<Result<Infallible>>>,
    terminate_sender: Option<oneshot::Sender<()>>,
    is_failed: Arc<AtomicBool>,
}
impl Browser {
    pub async fn new() -> Result<Self> {
        let connection = Connection::system().await?;
        let server = Server2Proxy::builder(&connection)
            .destination(DESTINATION)?
            .path("/")?
            .build()
            .await?;
        let browser_path = server
            .service_browser_prepare(-1, -1, SERVICE_TYPE, "", 0)
            .await?;
        let browser = ServiceBrowserProxy::builder(&connection)
            .destination(DESTINATION)?
            .path(browser_path)?
            .build()
            .await?;

        let current_items = Arc::new(Mutex::new(HashSet::new()));
        let mut callback_handles = Vec::new();

        let mut item_new = browser.receive_item_new().await?;
        let h = spawn({
            let current_items = Arc::clone(&current_items);
            async move {
                while let Some(item) = item_new.next().await {
                    let item = item.args()?;
                    let hostname = item.name.to_string();
                    current_items.lock().unwrap().insert(HostInfo { hostname });
                }
                || -> Result<Infallible> { unreachable!() }()
            }
        });
        callback_handles.push(h);

        let mut item_remove = browser.receive_item_remove().await?;
        let h = spawn({
            let current_items = Arc::clone(&current_items);
            async move {
                while let Some(item) = item_remove.next().await {
                    let item = item.args()?;
                    let hostname = item.name.to_string();
                    current_items.lock().unwrap().remove(&HostInfo { hostname });
                }
                || -> Result<Infallible> { unreachable!() }()
            }
        });
        callback_handles.push(h);

        let mut all_for_now = browser.receive_all_for_now().await?;
        let (change_sender, change_receiver) = mpsc::channel(1);
        let h = spawn({
            async move {
                while let Some(_) = all_for_now.next().await {
                    let _ = change_sender.try_send(());
                }
                || -> Result<Infallible> { unreachable!() }()
            }
        });
        callback_handles.push(h);
        let is_failed = Arc::new(AtomicBool::new(false));
        let mut on_failure = browser.receive_failure().await?;
        let h = spawn({
            let is_failed = Arc::clone(&is_failed);
            async move {
                while let Some(_) = on_failure.next().await {
                    is_failed.store(true, Ordering::SeqCst);
                }
                || -> Result<Infallible> { unreachable!() }()
            }
        });
        callback_handles.push(h);

        browser.start().await?;

        let (terminate_sender, terminate_receiver) = oneshot::channel();
        spawn(async move {
            terminate_receiver.await.unwrap();
            browser.free().await.unwrap();
        });

        Ok(Self {
            current_items,
            change_receiver,
            callback_handles,
            terminate_sender: Some(terminate_sender),
            is_failed,
        })
    }
    pub fn get_current_items(&self) -> Result<Vec<HostInfo>> {
        ensure!(!self.is_failed.load(Ordering::SeqCst), "Browser is failed");
        let current_items = self.current_items.lock().unwrap();
        Ok(current_items.iter().cloned().collect())
    }
    pub async fn get_updated_items(&mut self) -> Result<Vec<HostInfo>> {
        ensure!(!self.is_failed.load(Ordering::SeqCst), "Browser is failed");
        self.change_receiver.recv().await;
        self.get_current_items()
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        for handle in self.callback_handles.drain(..) {
            handle.abort();
        }
        self.terminate_sender.take().unwrap().send(()).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anyhow::Result;

    use tokio::{
        process::{Child, Command},
        time::sleep,
    };

    use crate::{
        location,
        neighbor_discovery::{
            SERVICE_TYPE,
            browse::{Browser, HostInfo},
            test::generate_random_hostname,
        },
    };

    async fn advertise_with_command(hostname: &str, port: u16) -> Result<Child> {
        let cmd = Command::new("avahi-publish")
            .arg("-s")
            .arg(hostname)
            .arg(SERVICE_TYPE)
            .arg(port.to_string())
            .kill_on_drop(true)
            .spawn()?;
        anyhow::Ok(cmd)
    }
    #[tokio::test]
    async fn test_browse() -> Result<()> {
        let hostname = generate_random_hostname(location!());
        let mut _c = advertise_with_command(&hostname, 8080).await?;
        sleep(Duration::from_secs(1)).await;
        assert!(
            Browser::new()
                .await?
                .get_updated_items()
                .await?
                .contains(&HostInfo { hostname })
        );
        Ok(())
    }
    #[tokio::test]
    async fn test_browse_failure() -> Result<()> {
        let hostname0 = generate_random_hostname(location!());
        let hostname1 = generate_random_hostname(location!());
        let mut _c = advertise_with_command(&hostname0, 8080).await?;
        sleep(Duration::from_secs(1)).await;
        assert!(
            !Browser::new()
                .await?
                .get_updated_items()
                .await?
                .contains(&HostInfo {
                    hostname: hostname1
                })
        );
        Ok(())
    }
}
