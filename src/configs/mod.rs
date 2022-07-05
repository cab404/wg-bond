use crate::wg_tools;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// Which networks to proxy
    pub networks: Vec<IpNetwork>,
    /// Whether to proxy whole internet, except [local networks](https://en.wikipedia.org/wiki/Private_network)
    /// Useful on mobile devices. Can be redundant.
    /// See also [`GLOBAL_NET_V4`] and [`GLOBAL_NET_V6`]
    pub use_global_networks: bool,
    /// Whether to allow to connect to whole internet
    pub proxy_internet: bool,
}

// Describes emergent features of peers, not set by one flag.
#[derive(Serialize, Deserialize, Debug, AsRefStr, Clone, PartialEq, Eq)]
pub enum PeerFlag {
    Masquerade { interface: String },
    Gateway { ignore_local_networks: bool },
    UseGateway { peer: u128, proxy: ProxyConfig },
    Segment { mask: u128 },
    Keepalive { keepalive: u16 },
    DNS { addresses: Vec<IpAddr> },
    NixOpsMachine,
    Center,
    Template,
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
        match &self {
            &PeerFlag::UseGateway { proxy, peer } => {
                let target_peer = network.by_id(*peer).unwrap();
            }
            &PeerFlag::Gateway {
                ignore_local_networks,
            } => {
                let has_ipv4 = network.networks.iter().any(IpNetwork::is_ipv4);
                let has_ipv6 = network.networks.iter().any(IpNetwork::is_ipv6);

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
            &PeerFlag::Center => {
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
    pub ips: Vec<IpAddr>,
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

    pub fn is_template(&self) -> bool {
        self.flags.contains(&PeerFlag::Template)
    }
}

// Overall network informatiom
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WireguardNetworkInfo {
    pub name: String,
    pub flags: Vec<NetworkFlag>,
    pub networks: Vec<IpNetwork>,
    pub peers: Vec<PeerInfo>,
    // Non-overlapping ignored subnets
    pub ignored_ipv4: HashSet<Ipv4Network>,
    pub ignored_ipv6: HashSet<Ipv6Network>,
}

#[derive(Serialize, Deserialize, Debug, AsRefStr, Clone)]
pub enum NetworkFlag {
    Centralized,
    // TODO: Add symmetric keys overlay
}

/// Searches for an item matching given pattern
macro_rules! find_pattern {
    ($self:expr => $pat:pat) => {
        $self.iter().find(|t| match t {
            $pat => true,
            _ => false,
        })
    };
}

impl WireguardNetworkInfo {
    pub fn map_to_peer(&self, info: &PeerInfo) -> Result<Peer, String> {
        let mut peer = info.derive_peer()?;
        peer.allowed_ips = info.ips.iter().map(|n| as_network(*n)).collect();

        for flag in &info.flags {
            flag.apply_to_peer(self, &mut peer)
        }
        Ok(peer)
    }

    pub fn map_to_interface(&self, info: &PeerInfo) -> Result<Interface, String> {
        if info.is_template() {
            Err("Cannot generate interface for template peer")?;
        }
        let mut interface = info.derive_interface()?;

        interface.address = info.ips.clone();

        for flag in &info.flags {
            flag.apply_to_interface(self, &mut interface)
        }
        Ok(interface)
    }

    /// Returns a list of peers for configuration of a given peer
    pub fn peer_list(&self, info: &PeerInfo) -> Vec<&PeerInfo> {
        let others = || {
            self.peers
                .iter()
                .filter(|peer| peer.id != info.id)
                .collect::<Vec<_>>()
        };

        if let Some(&PeerFlag::UseGateway { peer, .. }) =
            find_pattern!(info.flags => PeerFlag::UseGateway { .. })
        {
            // in this case we only need a gateway
            let gateway = self.by_id(peer).expect(
                format!(
                    "UseGateway flag on #{} points to nonexistent peer #{}!",
                    info.id, peer
                )
                .as_str(),
            );
            return vec![gateway];
        }

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
        let mut peers = self.peers.iter_mut();
        peers.find(|f| f.name == *name)
    }

    pub fn by_name(&self, name: &str) -> Option<&PeerInfo> {
        let mut peers = self.peers.iter();
        peers.find(|f| f.name == *name)
    }

    pub fn by_id(&self, id: u128) -> Option<&PeerInfo> {
        self.peers.iter().find(|f| f.id == id)
    }

    pub fn has_flag(&self, flag_name: &str) -> bool {
        self.flags.iter().any(|f| f.as_ref() == flag_name)
    }

    fn first_unignored_ipv4(&self, ip: Ipv4Addr, net: Ipv4Network) -> Option<Ipv4Addr> {
        let mut ip = ip;
        while let Some(n) = self.ignored_ipv4.iter().find(|n| n.contains(ip.into())) {
            // This way we can only increase IP
            // because overlaps => end of range is greater
            ip = u32::from(n.ip()).checked_add(n.size())?.into();
        }
        if net.contains(ip) {
            Some(ip)
        } else {
            None
        }
    }

    fn first_unignored_ipv6(&self, ip: Ipv6Addr, net: Ipv6Network) -> Option<Ipv6Addr> {
        let mut ip = ip;
        while let Some(n) = self.ignored_ipv6.iter().find(|n| n.contains(ip.into())) {
            ip = u128::from(n.ip()).checked_add(n.size())?.into();
        }
        if net.contains(ip) {
            Some(ip)
        } else {
            None
        }
    }

    pub fn assigned_ips(&self) -> HashSet<IpAddr> {
        self.peers
            .iter()
            .flat_map(|peer| peer.ips.clone())
            .collect()
    }

    /// Allocate free IP in specified network
    pub fn get_free_net_address(&self, net: IpNetwork) -> Result<IpAddr, String> {
        let is_ipv6 = net.is_ipv6();
        let net_ip = net.ip();
        let ip = self
            .assigned_ips()
            .into_iter()
            .filter(|ip| ip.is_ipv4() && !is_ipv6 || ip.is_ipv6() && is_ipv6)
            .max()
            .unwrap_or_else(|| net_ip);

        match (ip, net) {
            (IpAddr::V4(ip), IpNetwork::V4(net)) => next_ipv4(ip)
                .and_then(|ip| self.first_unignored_ipv4(ip, net))
                .map(IpAddr::V4),
            (IpAddr::V6(ip), IpNetwork::V6(net)) => next_ipv6(ip)
                .and_then(|ip| self.first_unignored_ipv6(ip, net))
                .map(IpAddr::V6),
            _ => panic!("Internal error"),
        }
        .ok_or(std::format!("No more unreserved IPs left in {}", net))
    }
}

fn next_ipv4(ip: Ipv4Addr) -> Option<Ipv4Addr> {
    u32::from(ip).checked_add(1).map(u32::into)
}

fn next_ipv6(ip: Ipv6Addr) -> Option<Ipv6Addr> {
    u128::from(ip).checked_add(1).map(u128::into)
}

pub fn as_network(addr: IpAddr) -> IpNetwork {
    match addr {
        IpAddr::V4(_) => IpNetwork::new(addr, 32).unwrap(),
        IpAddr::V6(_) => IpNetwork::new(addr, 128).unwrap(),
    }
}

pub trait ConfigType {
    type ExportConfig;
    // let config = net.get_configuration(my_peer);
    // let interface = net.map_to_interface(my_peer);
    fn write_config(net: WireguardConfiguration, options: Self::ExportConfig) -> String;
}

pub trait IpNetDifference: Sized + core::hash::Hash + std::cmp::Eq {
    fn subtract(&self, other: &Self) -> HashSet<Self>;
    fn subnets(&self) -> (Self, Self);

    fn subtract_all(minuend: &HashSet<Self>, subtrahend: &Self) -> HashSet<Self> {
        minuend
            .iter()
            .flat_map(|n| n.subtract(subtrahend))
            .collect()
    }
}

impl IpNetDifference for Ipv4Network {
    fn subtract(&self, other: &Self) -> HashSet<Self> {
        use std::cmp;

        let min_pref = cmp::min(self.prefix(), other.prefix());
        let prefs_equal =
            first_nbits32(self.ip().into(), min_pref) == first_nbits32(other.ip().into(), min_pref);
        if other.prefix() == min_pref && prefs_equal {
            HashSet::new()
        } else if !prefs_equal {
            HashSet::from_iter([*self])
        } else {
            let mut filtered: HashSet<Self> = HashSet::new();
            let (n1, n2) = self.subnets();
            filtered.extend(&n1.subtract(other));
            filtered.extend(&n2.subtract(other));
            filtered
        }
    }

    fn subnets(&self) -> (Self, Self) {
        let new_prefix = self.prefix() + 1;
        let first = u32::from(self.ip()) & !(1 << (32 - new_prefix));
        let second = u32::from(self.ip()) | (1 << (32 - new_prefix));
        let to_net = |addr: u32| Ipv4Network::new(addr.into(), new_prefix).unwrap();
        (to_net(first), to_net(second))
    }
}

impl IpNetDifference for Ipv6Network {
    fn subtract(&self, other: &Self) -> HashSet<Self> {
        use std::cmp;

        let min_pref = cmp::min(self.prefix(), other.prefix());
        let prefs_equal = first_nbits128(self.ip().into(), min_pref)
            == first_nbits128(other.ip().into(), min_pref);
        if other.prefix() == min_pref && prefs_equal {
            HashSet::new()
        } else if !prefs_equal {
            HashSet::from_iter([*self])
        } else {
            let mut filtered: HashSet<Self> = HashSet::new();
            let (n1, n2) = self.subnets();
            filtered.extend(&n1.subtract(other));
            filtered.extend(&n2.subtract(other));
            filtered
        }
    }

    fn subnets(&self) -> (Self, Self) {
        let new_prefix = self.prefix() + 1;
        let first = u128::from(self.ip()) & !(1 << (128 - new_prefix));
        let second = u128::from(self.ip()) | (1 << (128 - new_prefix));
        let to_net = |addr: u128| Ipv6Network::new(addr.into(), new_prefix).unwrap();
        (to_net(first), to_net(second))
    }
}

fn first_nbits32(x: u32, n: u8) -> u32 {
    x & (u32::MAX << (32 - n))
}

fn first_nbits128(x: u128, n: u8) -> u128 {
    x & (u128::MAX << (128 - n))
}
