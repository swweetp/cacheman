use std::mem::forget;

use anyhow::Result;
use tokio::{spawn, sync::oneshot};
use zbus::Connection;

use super::{
    DESTINATION, SERVICE_TYPE,
    zbus_binding::{entry_group::EntryGroupProxy, server2::Server2Proxy},
};

pub struct Advertiser {
    sender: oneshot::Sender<()>,
}
impl Advertiser {
    pub async fn new(hostname: &str, port: u16) -> Result<Self> {
        let connection = Connection::system().await?;
        let server = Server2Proxy::builder(&connection)
            .destination(DESTINATION)?
            .path("/")?
            .build()
            .await?;
        let entry_group_path = server.entry_group_new().await?;
        let entry_group = EntryGroupProxy::builder(&connection)
            .destination(DESTINATION)?
            .path(entry_group_path)?
            .build()
            .await?;
        entry_group
            .add_service(-1, -1, 0, hostname, SERVICE_TYPE, "", "", port, &[])
            .await?;
        entry_group.commit().await?;
        let (sender, receiver) = oneshot::channel();
        spawn(async move {
            if receiver.await.is_err() {
                forget(connection);
            }
        });
        Ok(Self { sender })
    }
    pub fn terminate(self) {
        self.sender.send(()).unwrap()
    }
    #[must_use]
    pub fn terminate_handle(self) -> AdvertiserTerminateHandle {
        AdvertiserTerminateHandle(Some(self))
    }
}

pub struct AdvertiserTerminateHandle(Option<Advertiser>);
impl Drop for AdvertiserTerminateHandle {
    fn drop(&mut self) {
        self.0.take().unwrap().terminate();
    }
}

#[cfg(test)]
mod tests {
    use std::{str::from_utf8, time::Duration};

    use anyhow::ensure;
    use tokio::{process::Command, task::yield_now, time::sleep};

    use crate::{location, neighbor_discovery::test::generate_random_hostname};

    use super::*;

    async fn browse_with_command(hostname: &str) -> Result<bool> {
        let command = Command::new("avahi-browse")
            .arg("--terminate")
            .arg("--parsable")
            .arg(SERVICE_TYPE)
            .output()
            .await?;
        ensure!(
            command.status.success(),
            "avahi-browse failed: {}",
            String::from_utf8_lossy(&command.stderr),
        );
        let output = from_utf8(&command.stdout)?;

        let is_exists = output.lines().any(|line| {
            let rows = line.split(';').collect::<Vec<_>>();
            rows[0] == "+" && rows[3] == hostname && rows[4] == SERVICE_TYPE
        });

        Ok(is_exists)
    }

    #[tokio::test]
    async fn test_advertise() -> Result<()> {
        let hostname = generate_random_hostname(location!().as_str());

        Advertiser::new(&hostname, 8080).await?;
        sleep(Duration::from_secs(1)).await;
        assert!(browse_with_command(&hostname).await?);
        yield_now().await;
        Ok(())
    }
    #[tokio::test]
    async fn test_advertise_failure() -> Result<()> {
        let hostname0 = generate_random_hostname(location!().as_str());
        let hostname1 = generate_random_hostname(location!().as_str());

        Advertiser::new(&hostname0, 8080).await?;
        sleep(Duration::from_secs(1)).await;
        assert!(!browse_with_command(&hostname1).await?);
        Ok(())
    }
    #[tokio::test]
    async fn terminate_handle() -> Result<()> {
        let hostname = generate_random_hostname(location!().as_str());
        let h = Advertiser::new(&hostname, 8080).await?.terminate_handle();
        sleep(Duration::from_secs(1)).await;
        drop(h);
        Ok(())
    }
}
