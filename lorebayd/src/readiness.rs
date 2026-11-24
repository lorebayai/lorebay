use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use reqwest::{Client, StatusCode, redirect::Policy};

use crate::Config;

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Checks HTTP readiness without opening storage or starting a server.
pub async fn readiness(config: &Config) -> Result<()> {
    let mut address = config.listen_address;
    if address.ip().is_unspecified() {
        address.set_ip(match address.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        });
    }
    let response = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(PROBE_TIMEOUT)
        .build()?
        .get(format!("http://{address}/readyz"))
        .send()
        .await
        .context("could not request /readyz")?;
    ensure!(
        response.status() == StatusCode::OK,
        "server is not ready: {}",
        response.status()
    );
    Ok(())
}
