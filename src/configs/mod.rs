use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use crate::wg_tools;
use std::iter::*;
use std::str::FromStr;
use url::Host;
use strum_macros::AsRefStr;

pub mod conf;
pub mod nix;
pub mod qr;

const GLOBAL_NET: &[&str; 30] = &[
    "0.0.0.0/5",
    "8.0.0.0/7",
    "11.0.0.0/8",
    "12.0.0.0/6",
    "16.0.0.0/4",
    "32.0.0.0/3",
    "64.0.0.0/2",
    "128.0.0.0/3",
    "160.0.0.0/5",
    "168.0.0.0/6",
    "172.0.0.0/12",
    "172.32.0.0/11",
    "172.64.0.0/10",
    "172.128.0.0/9",
    "173.0.0.0/8",
    "174.0.0.0/7",
    "176.0.0.0/4",
    "192.0.0.0/9",
    "192.128.0.0/11",
    "192.160.0.0/13",
    "192.169.0.0/16",
    "192.170.0.0/15",
    "192.172.0.0/14",
    "192.176.0.0/12",
    "192.192.0.0/10",
    "193.0.0.0/8",
    "194.0.0.0/7",
    "196.0.0.0/6",
    "200.0.0.0/5",
    "208.0.0.0/4",
];

/// Checks if endpoint is a valid ip or domain, and extracts port from it.
/// ```
/// assert_eq!(parse_url("test:8080"), Some((Host::Domain("test".to_string()), 8080)));
/// ```
fn split_endpoint(address: &String) -> Option<(Host, u16)> {
    let split = address.rsplitn(2, ':').collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>();
    if split.len() == 2 {
        if let (Some(host), Some(port)) = (
                Host::parse(split[0]).map(Some).unwrap_or(None), 
                u16::from_str(split[1]).map(Some).unwrap_or(None)
            ) {
                Some((host, port))
            } else {
                None
            }

    } else {
        None
    }
}
#[test]
fn test_parse_endpoint() {
    assert_eq!(split_endpoint(&"test:8080".into()), Some((Host::Domain("test".into()), 8080)));
    assert_eq!(split_endpoint(&"@:8080".into()), None);
}

pub fn get_port(address: &String) -> u16 {
    split_endpoint(address).expect("Failed to parse endpoint!").1
}

#[test]
pub fn test_get_port() {
    assert_eq!(get_port(&"test:8080".into()), 8080);
}

/// Checks whether given string is a valid endpoint
pub fn check_endpoint(address: &String) -> Option<String> {
    if let Some(_) = split_endpoint(address) {
        Some(address.clone())
    } else {
        None
    }
}

#[test]
pub fn test_check_endpoint() {
    assert_eq!(check_endpoint(&"test:8080".into()), Some("test:8080".into()));
    assert_eq!(check_endpoint(&"test::".into()), None);
}

// Mapping of wg-quick interface.
#[derive(Serialize, Deserialize, Debug)]
pub struct Interface {
    pub private_key: String,
    pub address: Vec<IpAddr>,
    pub port: Option<u16>,
    pub dns: Vec<IpAddr>,
    pub fw_mark: Option<u32>,
    pub table: Option<String>,
    pub pre_up: Option<String>,
    pub post_up: Option<String>,
    pub pre_down: Option<String>,
    pub post_down: Option<String>,
}

// Mapping of wg-quick peer.
#[derive(Serialize, Deserialize, Debug)]
pub struct Peer {
    pub public_key: String,
    pub preshared_key: Option<String>,
    pub allowed_ips: Vec<IpNetwork>,
    pub endpoint: Option<String>,
    pub persistent_keepalive: Option<u16>,
}

// Describes emergent features of peers, not set by one flag.
#[derive(Serialize, Deserialize, Debug, AsRefStr)]
pub enum PeerFlag {
    Masquerade { interface: String },
    Gateway { ignore_local_networks: bool },
    Keepalive { keepalive: u16 }
}

#[test]
fn test_flags_to_string() {
    let a = PeerFlag::Masquerade { interface: "test".to_string() };
    assert_eq!(a.as_ref(), "Masquerade")
}

impl PeerFlag {
    fn apply_to_interface(&self, network: &WireguardNetworkInfo, interface: &mut Interface) {
        match self {
            PeerFlag::Masquerade { interface: if_name } => {
                let iptables_bring_up = format!("iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}", "-A", &network.network, if_name); 
                let iptables_bring_down = format!("iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}", "-D", &network.network, if_name); 

                interface.pre_up = Some(iptables_bring_up.to_string());
                interface.pre_down = Some(iptables_bring_down.to_string());
            }
            _ => {}
        }
    }

    fn apply_to_peer(&self, _network: &WireguardNetworkInfo, peer: &mut Peer) {
        match self {
            PeerFlag::Gateway { ignore_local_networks } => {
                if *ignore_local_networks {
                    peer.allowed_ips.append(
                        &mut GLOBAL_NET.iter().map(|a| IpNetwork::from_str(a).unwrap()).collect()
                    )
                }
            }
            PeerFlag::Keepalive { keepalive } => {
                peer.persistent_keepalive = Some(*keepalive)
            }
            _ => {}
        }    
    }
}

// Information about a peer
#[derive(Serialize, Deserialize, Debug)]
pub struct PeerInfo {
    pub name: Option<String>,
    pub private_key: String,
    pub id: u128,
    pub flags: Vec<PeerFlag>,
    pub endpoint: Option<String>
}

impl PeerInfo {

    pub fn derive_interface(&self) -> Interface {
        Interface {
            address: vec![],
            private_key: self.private_key.clone(),
            port: self.endpoint.as_ref().map(get_port),
            dns: vec![],
            fw_mark: None,
            table: None,
            pre_up: None,
            post_up: None,
            pre_down: None,
            post_down: None
        }
    }

    pub fn derive_peer(&self) -> Peer {
        Peer {
            public_key: wg_tools::gen_public_key(&self.private_key),
            allowed_ips: vec![],
            endpoint: self.endpoint.clone(),
            persistent_keepalive: None,
            preshared_key: None
        }
    }

}

// Overall network informatiom
#[derive(Serialize, Deserialize, Debug)]
pub struct WireguardNetworkInfo {
    pub name: String,
    pub network: IpNetwork,
    pub peers: Vec<PeerInfo>
}

impl WireguardNetworkInfo {

    pub fn map_to_peer(&self, info: &PeerInfo) -> Peer {
        let mut peer = info.derive_peer();
        peer.allowed_ips = vec![
            IpNetwork::new(get_network_address(&self.network, info.id), self.network.prefix()).unwrap()
        ];

        for flag in &info.flags {
            flag.apply_to_peer(self, &mut peer)
        }
        peer
    }

    pub fn map_to_interface(&self, info: &PeerInfo) -> Interface {
        let mut interface = info.derive_interface();

        interface.address = vec![
            get_network_address(&self.network, info.id)
        ];

        for flag in &info.flags {
            flag.apply_to_interface(self, &mut interface)
        }
        interface
    }

    pub fn by_id(&self, id: u128) -> Option<&PeerInfo> {
        for peer in self.peers.iter() {
            if peer.id == id {
                return Some(peer)
            }
        }
        return None
    }
}

fn get_network_address_v4(net: &Ipv4Network, num: u32) -> Ipv4Addr {
    assert!(net.size() > num);
    Ipv4Addr::from(u32::from_be_bytes(net.ip().octets().clone()) | (num & (!0u32 >> net.prefix())))
}

fn get_network_address_v6(net: &Ipv6Network, num: u128) -> Ipv6Addr {
    assert!(net.size() > num);
    Ipv6Addr::from(u128::from_be_bytes(net.ip().octets().clone()) | (num & (!0u128 >> net.prefix())))
}

pub fn get_network_address(net: &IpNetwork, num: u128) -> IpAddr {
    match &net {
        IpNetwork::V4(n) => IpAddr::V4(get_network_address_v4(&n, num.try_into().unwrap())),
        IpNetwork::V6(n) => IpAddr::V6(get_network_address_v6(&n, num.try_into().unwrap())),
    }
}

pub type ConfigWriter = fn(net: &WireguardNetworkInfo, id: u128) -> String;

pub trait ConfigType {
    fn write_config(net: &WireguardNetworkInfo, id: u128) -> String;
}
