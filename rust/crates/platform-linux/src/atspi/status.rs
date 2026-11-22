// SPDX-License-Identifier: MIT
//! Probe the actual accessibility bus and registry, not just an environment
//! variable or an introspectable launcher. D-Bus activation requires no prompt.
use anyhow::{Context, Result};
use std::time::Duration;

pub const SETUP_HINT: &str = "NixOS: add services.gnome.at-spi2-core.enable = true; (also for Plasma), then log in again. Other Linux: install at-spi2-core with its org.a11y.Bus D-Bus service. For Qt apps without a tree, launch them with QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1. Coordinate input and screenshots do not require AT-SPI.";

pub async fn enable_and_connect() -> Result<zbus::Connection> {
    let session = zbus::Connection::session().await?;
    let launcher =
        zbus::Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").await?;
    let address: String = launcher
        .call("GetAddress", &())
        .await
        .context("org.a11y.Bus.GetAddress failed")?;
    let status =
        zbus::Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status").await?;
    if matches!(status.get_property::<bool>("IsEnabled").await, Ok(false)) {
        status
            .set_property("IsEnabled", true)
            .await
            .context("could not enable AT-SPI")?;
    }
    if let Some(preferred) = std::env::var("AT_SPI_BUS_ADDRESS")
        .ok()
        .filter(|v| !v.is_empty())
    {
        if let Ok(Ok(connection)) =
            tokio::time::timeout(Duration::from_millis(800), registry_connection(&preferred)).await
        {
            return Ok(connection);
        }
    }
    registry_connection(&address).await
}

async fn registry_connection(address: &str) -> Result<zbus::Connection> {
    let connection = zbus::connection::Builder::address(address)?.build().await?;
    // at-spi2-registryd supports Accessible.GetChildren but does not implement
    // Properties.GetAll for this interface on every release. Do not mistake
    // that optional property-cache failure for an unreachable registry.
    let registry: zbus::Proxy<'_> = zbus::proxy::Builder::new(&connection)
        .destination("org.a11y.atspi.Registry")?
        .path("/org/a11y/atspi/accessible/root")?
        .interface("org.a11y.atspi.Accessible")?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let _: Vec<(String, zbus::zvariant::OwnedObjectPath)> = registry
        .call("GetChildren", &())
        .await
        .context("AT-SPI registry did not answer")?;
    Ok(connection)
}

pub fn probe() -> Result<String> {
    std::thread::spawn(|| -> Result<String> {
        tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
            tokio::time::timeout(Duration::from_secs(3), async {
                let _ = enable_and_connect().await?;
                let session = zbus::Connection::session().await?;
                let status = zbus::Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Status").await?;
                let enabled = status.get_property::<bool>("IsEnabled").await.ok();
                let reader = status.get_property::<bool>("ScreenReaderEnabled").await.ok();
                Ok(format!("accessibility bus and registry reachable; IsEnabled={enabled:?}, ScreenReaderEnabled={reader:?}; QT_LINUX_ACCESSIBILITY_ALWAYS_ON={}",
                    std::env::var("QT_LINUX_ACCESSIBILITY_ALWAYS_ON").unwrap_or_else(|_| "unset (set to 1 when launching Qt apps that expose no tree)".into())))
            }).await.context("AT-SPI probe timed out")?
        })
    }).join().map_err(|_| anyhow::anyhow!("AT-SPI probe panicked"))?
}
