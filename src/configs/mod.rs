use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use crate::wg_tools;
use std::iter::*;
use std::str::FromStr;

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

// Mapping of wg-quick interface.
#[derive(Serialize, Deserialize, Debug)]
pub struct Interface {
    pub private_key: String,
    pub address: Vec<IpAddr>,
    pub port: Option<u16>,
    pub dns: Option<IpAddr>,
    pub fwMark: Option<u32>,
    pub table: Option<String>,
    pub preUp: Option<String>,
    pub postUp: Option<String>,
    pub preDown: Option<String>,
    pub postDown: Option<String>,
}

// Mapping of wg-quick peer.
#[derive(Serialize, Deserialize, Debug)]
pub struct Peer {
    pub public_key: String,
    pub preshared_key: Option<String>,
    pub allowed_ips: Vec<IpNetwork>,
    pub endpoint: Vec<SocketAddr>,
    pub persistent_keepalive: Option<u16>,
}

// Describes emergent features of peers, not set by one flag.
#[derive(Serialize, Deserialize, Debug)]
pub enum PeerFlag {
    Masquerade { interface: String },
    Gateway { ignore_local_networks: bool },
    Keepalive { keepalive: u16 }
}

impl PeerFlag {
    fn apply_to_interface(&self, network: &WireguardNetworkInfo, interface: &mut Interface) {
        match self {
            PeerFlag::Masquerade { interface: if_name } => {
                let iptables_bring_up = format!("iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}", "-A", &network.network, if_name); 
                let iptables_bring_down = format!("iptables {} POSTROUTING -t nat -j MASQUERADE -s {} -o {}", "-D", &network.network, if_name); 

                interface.preUp = Some(iptables_bring_up.to_string());
                interface.preDown = Some(iptables_bring_down.to_string());
            }
            _ => {}
        }
    }

    fn apply_to_peer(&self, network: &WireguardNetworkInfo, peer: &mut Peer) {
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
    pub port: Option<u16>,
    pub addresses: Vec<IpAddr>
}

impl PeerInfo {

    fn is_gateway(&self) -> bool {
        self.flags.iter().any(
            |a| match a {
                PeerFlag::Masquerade { interface } => { true }
                _ => { false }
            }
        )
    }

    pub fn derive_interface(&self) -> Interface {
        Interface {
            address: vec![],
            private_key: self.private_key.clone(),
            port: self.port,
            dns: None,
            fwMark: None,
            table: None,
            preUp: None,
            postUp: None,
            preDown: None,
            postDown: None
        }
    }

    pub fn derive_peer(&self) -> Peer {
        // If port is obtained dynamically, we can't get a proper endpoint.
        let addresses = 
            match self.port {
                None => {
                    vec![]
                }
                Some(port) => {
                    self.addresses
                        .iter()
                        .map(|a| SocketAddr::new(a.clone(), port))
                        .collect()
                }
            };
        Peer {
            public_key: wg_tools::gen_public_key(&self.private_key),
            allowed_ips: vec![],
            endpoint: addresses,
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

pub trait ConfigType {
    fn write_config(net: &WireguardNetworkInfo, id: u128) -> String;
}
