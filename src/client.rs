use std::{
    path::Path,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use http::{HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use zerotier_central_api::{types::Member, Client};
use zerotier_one_api::types::Network;

// address of Central
const CENTRAL_BASEURL: &str = "https://my.zerotier.com/api/v1";

// this provides the production configuration for talking to central through the openapi libraries.
pub fn central_client(token: String) -> Result<zerotier_central_api::Client, anyhow::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("bearer {}", token))?,
    );

    Ok(zerotier_central_api::Client::new_with_client(
        &std::env::var("ZEROTIER_CENTRAL_INSTANCE").unwrap_or(CENTRAL_BASEURL.to_string()),
        reqwest::Client::builder()
            .https_only(true)
            .default_headers(headers)
            .build()?,
    ))
}

// determine the path of the authtoken.secret
pub fn authtoken_path(arg: Option<&Path>) -> &Path {
    if let Some(arg) = arg {
        return arg;
    }

    if cfg!(target_os = "linux") {
        Path::new("/var/lib/zerotier-one/authtoken.secret")
    } else if cfg!(target_os = "windows") {
        Path::new("C:/ProgramData/ZeroTier/One/authtoken.secret")
    } else if cfg!(target_os = "macos") {
        Path::new("/Library/Application Support/ZeroTier/One/authtoken.secret")
    } else {
        panic!("authtoken.secret not found; please provide the -s option to provide a custom path")
    }
}

pub fn local_client_from_file(
    authtoken_path: &Path,
) -> Result<zerotier_one_api::Client, anyhow::Error> {
    let authtoken = std::fs::read_to_string(authtoken_path)?;
    local_client(authtoken)
}

fn local_client(authtoken: String) -> Result<zerotier_one_api::Client, anyhow::Error> {
    let mut headers = HeaderMap::new();
    headers.insert("X-ZT1-Auth", HeaderValue::from_str(&authtoken)?);

    Ok(zerotier_one_api::Client::new_with_client(
        "http://127.0.0.1:9993",
        reqwest::Client::builder()
            .default_headers(headers)
            .build()?,
    ))
}

pub async fn get_networks(s: mpsc::UnboundedSender<Vec<Network>>) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    let networks = client.get_networks().await?;

    s.send(networks.to_vec())?;
    Ok(())
}

pub async fn leave_network(network_id: String) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    Ok(*client.delete_network(&network_id).await?)
}

pub async fn join_network(network_id: String) -> Result<(), anyhow::Error> {
    let client = local_client_from_file(authtoken_path(None))?;
    client
        .update_network(
            &network_id,
            &Network {
                subtype_0: zerotier_one_api::types::NetworkSubtype0 {
                    allow_default: None,
                    allow_dns: None,
                    allow_global: None,
                    allow_managed: None,
                },
                subtype_1: zerotier_one_api::types::NetworkSubtype1 {
                    allow_default: None,
                    allow_dns: None,
                    allow_global: None,
                    allow_managed: None,
                    assigned_addresses: Vec::new(),
                    bridge: None,
                    broadcast_enabled: None,
                    dns: None,
                    id: None,
                    mac: None,
                    mtu: None,
                    multicast_subscriptions: Vec::new(),
                    name: None,
                    netconf_revision: None,
                    port_device_name: None,
                    port_error: None,
                    routes: Vec::new(),
                    status: None,
                    type_: None,
                },
            },
        )
        .await?;
    Ok(())
}

pub fn sync_get_networks() -> Result<Vec<Network>, anyhow::Error> {
    let (s, mut r) = mpsc::unbounded_channel();

    tokio::spawn(crate::client::get_networks(s));

    let networks: Vec<Network>;

    let timeout = Instant::now();

    'outer: loop {
        match r.try_recv() {
            Ok(n) => {
                networks = n;
                break 'outer;
            }

            Err(_) => std::thread::sleep(Duration::new(0, 10)),
        }

        if timeout.elapsed() > Duration::new(3, 0) {
            return Err(anyhow!("timeout reading from zerotier"));
        }
    }

    Ok(networks)
}

pub fn sync_get_members(client: Client, id: String) -> Result<Vec<Member>, anyhow::Error> {
    let (s, mut r) = mpsc::unbounded_channel();

    tokio::spawn(async move { s.send(client.get_network_member_list(&id).await).unwrap() });

    let members: Vec<Member>;

    let timeout = Instant::now();

    'outer: loop {
        match r.try_recv() {
            Ok(m) => match m {
                Ok(m) => {
                    members = m.to_vec();
                    break 'outer;
                }
                Err(e) => return Err(anyhow!(e)),
            },

            Err(_) => std::thread::sleep(Duration::new(0, 10)),
        }

        if timeout.elapsed() > Duration::new(3, 0) {
            return Err(anyhow!("timeout reading from zerotier"));
        }
    }

    Ok(members)
}
