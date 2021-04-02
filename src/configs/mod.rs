use crate::wg_tools;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::iter::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use strum_macros::AsRefStr;
use url::Host;

pub mod conf;
pub mod hosts;
pub mod nix;
pub mod nixops;
pub mod qr;

const GLOBAL_NET_V4: &[&str; 30] = &[
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

// not yet sure :/
const GLOBAL_NET_V6: &[&str; 1] = &["::/0"];

/// Checks if endpoint is a valid ip or domain, and extracts port from it.
/// ```
/// assert_eq!(parse_url("test:8080"), Some((Host::Domain("test".to_string()), 8080)));
/// ```
fn split_endpoint(address: String) -> Result<(Host, u16), String> {
    let split = address
        .rsplitn(2, ':')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    match split.len() {
        1 => Err("You forgot the port.".to_string()),
        2 => Ok((
            Host::parse(split[0]).map_err(|f| f.to_string())?,
            u16::from_str(split[1]).map_err(|_| "Port number is weird.")?,
        )),
        _ => panic!(),
    }
}
#[test]
fn test_parse_endpoint() {
    assert_eq!(
        split_endpoint("test:8080".to_string()),
        Ok((Host::Domain("test".into()), 8080))
    );
    assert_eq!(
        split_endpoint("@:8080".to_string()),
        Err("invalid domain character".to_string())
    );
}

pub fn get_port(address: String) -> Result<u16, String> {
    Ok(split_endpoint(address)?.1)
}

#[test]
pub fn test_get_port() {
    assert_eq!(get_port("test:8080".to_string()), Ok(8080));
}

/// Checks whether given string is a valid endpoint
pub fn check_endpoint(address: String) -> Result<String, String> {
    split_endpoint(address.clone()).map(|_| address)
}

#[test]
pub fn test_check_endpoint() {
    assert_eq!(
        check_endpoint("test:8080".to_string()),
        Ok("test:8080".to_string())
    );
    assert_eq!(
        check_endpoint("::test:".to_string()),
        Err("invalid domain character".to_string())
    );
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WireguardConfiguration {
    pub interface: Interface,
    pub peers: Vec<Peer>,
    pub name: String,
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
#[derive(Serialize, Deserialize, Debug, AsRefStr, Clone)]
pub enum PeerFlag {
    Masquerade { interface: String },
    Gateway { ignore_local_networks: bool },
    Keepalive { keepalive: u16 },
    DNS { addresses: Vec<IpAddr> },
    NixOpsMachine,
    Center,
}

#[test]
fn test_flags_to_string() {
    let a = PeerFlag::Masquerade {
        interface: "test".to_string(),
    };
    assert_eq!(a.as_ref(), "Masquerade")
}

impl PeerFlag {
    fn apply_to_interface(&self, network: &WireguardNetworkInfo, interface: &mut Interface) {
        match self {
            PeerFlag::Masquerade { interface: if_name } => {
                interface.pre_up = network
                    .networks
                    .iter()
                    .map(|f| match f {
                        IpNetwork::V4(n) => format!(
                            "iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}",
                            "-A", &n, if_name
                        ),
                        IpNetwork::V6(n) => format!(
                            "ip6tables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}",
                            "-A", &n, if_name
                        ),
                    })
                    .collect::<Vec<_>>()
                    .join(";")
                    .into();

                interface.pre_down = network
                    .networks
                    .iter()
                    .map(|f| match f {
                        IpNetwork::V4(n) => format!(
                            "iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}",
                            "-D", &n, if_name
                        ),
                        IpNetwork::V6(n) => format!(
                            "ip6tables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}",
                            "-D", &n, if_name
                        ),
                    })
                    .collect::<Vec<_>>()
                    .join(";")
                    .into();
            }
            PeerFlag::DNS { addresses } => {
                interface.dns = addresses.clone();
            }
            _ => {}
        }
    }

    fn apply_to_peer(&self, network: &WireguardNetworkInfo, peer: &mut Peer) {
        match self {
            PeerFlag::Gateway {
                ignore_local_networks,
            } => {
                let has_ipv4 = network.networks.iter().any(|f| {
                    if let IpNetwork::V4(_) = f {
                        true
                    } else {
                        false
                    }
                });
                let has_ipv6 = network.networks.iter().any(|f| {
                    if let IpNetwork::V6(_) = f {
                        true
                    } else {
                        false
                    }
                });

                if *ignore_local_networks {
                    let e: &[&str] = &[];
                    peer.allowed_ips.append(
                        &mut empty()
                            .chain(if has_ipv4 {
                                GLOBAL_NET_V4.iter()
                            } else {
                                e.iter()
                            })
                            .chain(if has_ipv6 {
                                GLOBAL_NET_V6.iter()
                            } else {
                                e.iter()
                            })
                            .map(|a| IpNetwork::from_str(a).unwrap())
                            .collect(),
                    )
                } else {
                    if has_ipv4 {
                        peer.allowed_ips
                            .insert(0, IpNetwork::from_str("0.0.0.0/0").unwrap())
                    }

                    if has_ipv6 {
                        peer.allowed_ips
                            .insert(0, IpNetwork::from_str("0::/0").unwrap())
                    }
                }
            }
            PeerFlag::Center => {
                for network in network.networks.iter().rev() {
                    peer.allowed_ips.insert(0, *network)
                }
            }
            _ => {}
        }
    }

    fn apply_to_configuration(
        &self,
        _network: &WireguardNetworkInfo,
        config: &mut WireguardConfiguration,
    ) {
        match self {
            PeerFlag::Keepalive { keepalive } => {
                for peer in config.peers.iter_mut() {
                    if peer.endpoint.is_some() {
                        peer.persistent_keepalive = Some(*keepalive);
                    }
                }
            }
            _ => {}
        }
    }
}

// Information about a peer
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PeerInfo {
    pub name: String,
    pub private_key: String,
    pub id: u128,
    pub flags: Vec<PeerFlag>,
    pub endpoint: Option<String>,
}

impl PeerInfo {
    pub fn has_flag(&self, flag_name: &str) -> bool {
        self.flags.iter().any(|f| f.as_ref() == flag_name)
    }

    pub fn derive_interface(&self) -> Result<Interface, String> {
        Ok(Interface {
            address: vec![],
            private_key: self.private_key.clone(),
            port: self.endpoint.clone().map(|f| get_port(f)).transpose()?,
            dns: vec![],
            fw_mark: None,
            table: None,
            pre_up: None,
            post_up: None,
            pre_down: None,
            post_down: None,
        })
    }

    pub fn derive_peer(&self) -> Result<Peer, String> {
        Ok(Peer {
            public_key: wg_tools::gen_public_key(&self.private_key)?,
            allowed_ips: vec![],
            endpoint: self.endpoint.clone(),
            persistent_keepalive: None,
            preshared_key: None,
        })
    }
}

// Overall network informatiom
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WireguardNetworkInfo {
    pub name: String,
    pub flags: Vec<NetworkFlag>,
    pub networks: Vec<IpNetwork>,
    pub peers: Vec<PeerInfo>,
}

#[derive(Serialize, Deserialize, Debug, AsRefStr, Clone)]
pub enum NetworkFlag {
    Centralized,
    // TODO: Add symmetric keys overlay
}

impl WireguardNetworkInfo {
    pub fn map_to_peer(&self, info: &PeerInfo) -> Result<Peer, String> {
        let mut peer = info.derive_peer()?;
        peer.allowed_ips = self
            .networks
            .iter()
            .map(|f| get_network_address_as_network(*f, info.id))
            .collect::<Vec<_>>();

        for flag in &info.flags {
            flag.apply_to_peer(self, &mut peer)
        }
        Ok(peer)
    }

    pub fn map_to_interface(&self, info: &PeerInfo) -> Result<Interface, String> {
        let mut interface = info.derive_interface()?;

        interface.address = self
            .networks
            .iter()
            .map(|f| get_network_address(*f, info.id))
            .collect::<Vec<_>>();

        for flag in &info.flags {
            flag.apply_to_interface(self, &mut interface)
        }
        Ok(interface)
    }

    pub fn peer_list(&self, info: &PeerInfo) -> Vec<&PeerInfo> {
        let others = || {
            self.peers
                .iter()
                .filter(|peer| peer.id != info.id)
                .collect::<Vec<_>>()
        };

        if self.has_flag("Centralized") {
            if info.has_flag("Center") {
                others()
            } else {
                self.peers
                    .iter()
                    .filter(|peer| peer.has_flag("Center"))
                    .collect::<Vec<_>>()
            }
        } else {
            others()
        }
    }

    pub fn get_configuration(&self, info: &PeerInfo) -> Result<WireguardConfiguration, String> {
        let mut config = WireguardConfiguration {
            interface: self.map_to_interface(info)?,
            peers: self
                .peer_list(info)
                .iter()
                .map(|x| self.map_to_peer(x))
                .collect::<Result<Vec<_>, _>>()?,
            name: self.name.clone(),
        };

        info.flags
            .iter()
            .for_each(|flag| flag.apply_to_configuration(self, &mut config));

        Ok(config)
    }

    pub fn by_name_mut(&mut self, name: &str) -> Option<&mut PeerInfo> {
        self.peers.iter_mut().find(|f| f.name == *name)
    }

    pub fn by_name(&self, name: &str) -> Option<&PeerInfo> {
        self.peers.iter().find(|f| f.name == *name)
    }

    pub fn has_flag(&self, flag_name: &str) -> bool {
        self.flags.iter().any(|f| f.as_ref() == flag_name)
    }
}

fn get_network_address_v4(net: Ipv4Network, num: u32) -> Ipv4Addr {
    assert!(net.size() > num);
    Ipv4Addr::from(u32::from_be_bytes(net.ip().octets()) | (num & (!0u32 >> net.prefix())))
}

fn get_network_address_v6(net: Ipv6Network, num: u128) -> Ipv6Addr {
    assert!(net.size() > num);
    Ipv6Addr::from(u128::from_be_bytes(net.ip().octets()) | (num & (!0u128 >> net.prefix())))
}

pub fn get_network_address_as_network(net: IpNetwork, num: u128) -> IpNetwork {
    match get_network_address(net, num) {
        a @ IpAddr::V4(_) => IpNetwork::new(a, 32).unwrap(),
        a @ IpAddr::V6(_) => IpNetwork::new(a, 128).unwrap(),
    }
}

pub fn get_network_address(net: IpNetwork, num: u128) -> IpAddr {
    match net {
        IpNetwork::V4(n) => IpAddr::V4(get_network_address_v4(n, num.try_into().unwrap())),
        IpNetwork::V6(n) => IpAddr::V6(get_network_address_v6(n, num.try_into().unwrap())),
    }
}

pub type ConfigWriter = fn(net: WireguardConfiguration) -> String;

pub trait ConfigType {
    // let config = net.get_configuration(my_peer);
    // let interface = net.map_to_interface(my_peer);
    fn write_config(net: WireguardConfiguration) -> String;
}
