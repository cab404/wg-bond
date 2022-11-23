use crate::find_pattern;
use crate::wg_tools;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use petgraph::graph::Graph;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::iter::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use strum_macros::AsRefStr;
use url::Host;
use utils::split_endpoint;

use self::ipop::as_network;
use self::utils::GLOBAL_NET_V4;
use self::utils::GLOBAL_NET_V6;
pub mod conf;
pub mod hosts;
mod ipop;
pub mod macros;
pub mod nix;
pub mod nixops;
pub mod qr;
mod relations;
mod utils;

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
    UseTemplate { peer: String },
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
}

#[derive(Serialize, Deserialize, Debug, AsRefStr, Clone)]
pub enum NetworkFlag {
    Centralized,
    IgnoredIPs {
        // Non-overlapping ignored subnets
        ignored_ipv4: HashSet<Ipv4Network>,
        ignored_ipv6: HashSet<Ipv6Network>,
    },
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
            self.real_peers()
                .into_iter()
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

    fn unfold_flags(&self, info: &PeerInfo) -> Result<PeerInfo, String> {
        let templates = self.collect_templates(info)?;
        let mut info = info.clone();

        // extend info's flags with those contained in templates
        for flag in templates
            .iter()
            .flat_map(|t| self.by_id(*t).unwrap().flags.iter())
        {
            info.flags.insert(0, flag.clone());
        }
        info.flags.retain(|f| match f {
            PeerFlag::UseTemplate { .. } => false,
            _ => true,
        });
        Ok(info)
    }

    pub fn get_configuration(&self, info: &PeerInfo) -> Result<WireguardConfiguration, String> {
        let info = self.unfold_flags(info)?;

        let mut config = WireguardConfiguration {
            interface: self.map_to_interface(&info)?,
            peers: self
                .peer_list(&info)
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

    pub fn by_id(&self, id: u128) -> Option<&PeerInfo> {
        self.peers.iter().find(|f| f.id == id)
    }

    pub fn has_flag(&self, flag_name: &str) -> bool {
        self.flags.iter().any(|f| f.as_ref() == flag_name)
    }

    pub fn real_peers(&self) -> Vec<&PeerInfo> {
        self.peers
            .iter()
            .filter(|p| !p.is_template())
            .collect::<Vec<_>>()
    }

    /// Recursively collect templates on which peer is dependent
    pub fn collect_templates(&self, peer: &PeerInfo) -> Result<Vec<u128>, String> {
        use petgraph::stable_graph::NodeIndex;
        use petgraph::visit::Dfs;

        let mut graph: Graph<u128, (), petgraph::Directed> = Graph::new();
        let mut peer_indices: HashMap<&String, NodeIndex<_>> = HashMap::new();

        // building map "peer_id -> graph node"
        for p in self.peers.iter() {
            peer_indices.insert(&p.name, graph.add_node(p.id));
        }

        for (peer, template) in self.peers.iter().flat_map(|p| {
            p.flags.iter().filter_map(move |f| {
                if let PeerFlag::UseTemplate { peer: template } = f {
                    Some((&p.name, template))
                } else {
                    None
                }
            })
        }) {
            let a = peer_indices[&peer];
            let b = peer_indices[&template];
            // let b = peer_indices
            //     .get(template)
            //     .ok_or(format!("No peer with name '{}' found", template))?;
            graph.add_edge(a, b, ());
        }

        if petgraph::algo::is_cyclic_directed(&graph) {
            return Err(String::from("Template dependencies contain a cycle"));
        }

        let mut dfs = Dfs::new(&graph, peer_indices[&peer.name]);
        let mut templates: Vec<u128> = Vec::new();
        while let Some(v) = dfs.next(&graph) {
            templates.push(*graph.node_weight(v).unwrap());
        }
        templates.retain(|p| *p != peer.id);
        Ok(templates)
    }
}
pub trait ConfigType {
    type ExportConfig;
    // let config = net.get_configuration(my_peer);
    // let interface = net.map_to_interface(my_peer);
    fn write_config(net: WireguardConfiguration, options: Self::ExportConfig) -> String;
}
