use std::{
    collections::HashSet,
    iter::FromIterator,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};

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

#[cfg(test)]
mod config_mod_test {
    use std::{net::IpAddr, str::FromStr};

    use crate::configs::PeerFlag;
    use crate::RVoid;

    use crate::tests::{add_peer, edit_peer, new_config};

    fn ip(ip: &str) -> IpAddr {
        IpAddr::from_str(ip).unwrap()
    }

    mod templates_test {
        use crate::configs::PeerInfo;

        use super::*;

        fn ensure_flags_contained(peer: &PeerInfo, flags: Vec<PeerFlag>) {
            for flag in flags {
                assert!(peer.flags.contains(&flag));
            }
        }

        #[test]
        fn test_simple() {
            (|| -> RVoid {
                let mut cfg = new_config(None);
                add_peer(&mut cfg, "DNS")?;
                edit_peer(&mut cfg, "DNS", "--dns 8.8.8.8")?;

                add_peer(&mut cfg, "peer")?;
                edit_peer(&mut cfg, "peer", "--use-template DNS --keepalive 30")?;

                edit_peer(&mut cfg, "DNS", "--dns 9.9.9.9")?;

                let peer = cfg.by_name("peer").unwrap();
                let peer = cfg.unfold_flags(peer)?;

                ensure_flags_contained(
                    &peer,
                    vec![
                        PeerFlag::DNS {
                            addresses: vec![ip("9.9.9.9")],
                        },
                        PeerFlag::Keepalive { keepalive: 30 },
                    ],
                );

                Ok(())
            })()
            .unwrap();
        }

        #[test]
        fn test_cycle() {
            (|| -> RVoid {
                let mut cfg = new_config(None);
                add_peer(&mut cfg, "a")?;
                add_peer(&mut cfg, "b")?;
                add_peer(&mut cfg, "c")?;

                edit_peer(&mut cfg, "a", "--use-template b")?;
                edit_peer(&mut cfg, "b", "--use-template c")?;
                edit_peer(&mut cfg, "c", "--use-template a")?;

                cfg.unfold_flags(cfg.by_name("a").unwrap())
                    .expect_err("expected cycle detection");
                cfg.unfold_flags(cfg.by_name("b").unwrap())
                    .expect_err("expected cycle detection");
                cfg.unfold_flags(cfg.by_name("c").unwrap())
                    .expect_err("expected cycle detection");

                Ok(())
            })()
            .unwrap();
        }

        #[test]
        fn test_two_layers() {
            (|| -> RVoid {
                let mut cfg = new_config(None);
                add_peer(&mut cfg, "DNS")?;
                add_peer(&mut cfg, "PHONE")?;
                add_peer(&mut cfg, "PC")?;
                add_peer(&mut cfg, "phone1")?;
                add_peer(&mut cfg, "pc1")?;

                edit_peer(&mut cfg, "DNS", "--dns 1.1.1.1")?;
                edit_peer(&mut cfg, "PHONE", "--keepalive 30 --use-template DNS")?;
                edit_peer(&mut cfg, "PC", "--center --use-template DNS")?;
                edit_peer(&mut cfg, "phone1", "--use-template PHONE")?;
                edit_peer(&mut cfg, "pc1", "--use-template PC")?;

                let phone1 = cfg.unfold_flags(cfg.by_name("phone1").unwrap())?;
                let pc1 = cfg.unfold_flags(cfg.by_name("pc1").unwrap())?;

                ensure_flags_contained(
                    &phone1,
                    vec![
                        PeerFlag::DNS {
                            addresses: vec![ip("1.1.1.1")],
                        },
                        PeerFlag::Keepalive { keepalive: 30 },
                    ],
                );

                ensure_flags_contained(
                    &pc1,
                    vec![
                        PeerFlag::DNS {
                            addresses: vec![ip("1.1.1.1")],
                        },
                        PeerFlag::Center,
                    ],
                );

                edit_peer(&mut cfg, "DNS", "--dns 8.8.8.8")?;
                edit_peer(&mut cfg, "PHONE", "--keepalive 100")?;
                edit_peer(&mut cfg, "PC", "--nixops")?;

                let phone1 = cfg.unfold_flags(cfg.by_name("phone1").unwrap())?;
                let pc1 = cfg.unfold_flags(cfg.by_name("pc1").unwrap())?;

                ensure_flags_contained(
                    &phone1,
                    vec![
                        PeerFlag::DNS {
                            addresses: vec![ip("8.8.8.8")],
                        },
                        PeerFlag::Keepalive { keepalive: 100 },
                    ],
                );

                ensure_flags_contained(
                    &pc1,
                    vec![
                        PeerFlag::DNS {
                            addresses: vec![ip("8.8.8.8")],
                        },
                        PeerFlag::Center,
                        PeerFlag::NixOpsMachine,
                    ],
                );

                assert!(!(pc1.has_flag("UseTemplate")));
                assert!(!(phone1.has_flag("UseTemplate")));

                Ok(())
            })()
            .unwrap();
        }
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

// fn first_unignored_ipv4(&self, ip: Ipv4Addr, net: Ipv4Network) -> Option<Ipv4Addr> {
//     let mut ip = ip;
//     if let Some(NetworkFlag::IgnoredIPs { ignored_ipv4, .. }) =
//         find_pattern!(self.flags => NetworkFlag::IgnoredIPs { .. })
//     {
//         while let Some(n) = ignored_ipv4.iter().find(|n| n.contains(ip.into())) {
//             // This way we can only increase IP
//             // because overlaps => end of range is greater
//             ip = u32::from(n.ip()).checked_add(n.size())?.into();
//         }
//         if net.contains(ip) {
//             Some(ip)
//         } else {
//             None
//         }
//     } else {
//         None
//     }
// }

// fn first_unignored_ipv6(&self, ip: Ipv6Addr, net: Ipv6Network) -> Option<Ipv6Addr> {
//     let mut ip = ip;
//     if let Some(NetworkFlag::IgnoredIPs { ignored_ipv6, .. }) =
//         find_pattern!(self.flags => NetworkFlag::IgnoredIPs { .. })
//     {
//         while let Some(n) = ignored_ipv6.iter().find(|n| n.contains(ip.into())) {
//             ip = u128::from(n.ip()).checked_add(n.size())?.into();
//         }
//         if net.contains(ip) {
//             Some(ip)
//         } else {
//             None
//         }
//     } else {
//         None
//     }
// }

// pub fn assigned_ips(&self) -> HashSet<IpAddr> {
//     self.peers
//         .iter()
//         .flat_map(|peer| peer.ips.clone())
//         .collect()
// }

// /// Allocate free IP in specified network
// pub fn get_free_net_address(&self, net: IpNetwork) -> Result<IpAddr, String> {
//     let is_ipv6 = net.is_ipv6();
//     let net_ip = net.ip();
//     let ip = self
//         .assigned_ips()
//         .into_iter()
//         .filter(|ip| ip.is_ipv4() && !is_ipv6 || ip.is_ipv6() && is_ipv6)
//         .max()
//         .unwrap_or_else(|| net_ip);

//     match (ip, net) {
//         (IpAddr::V4(ip), IpNetwork::V4(net)) => next_ipv4(ip)
//             .and_then(|ip| self.first_unignored_ipv4(ip, net))
//             .map(IpAddr::V4),
//         (IpAddr::V6(ip), IpNetwork::V6(net)) => next_ipv6(ip)
//             .and_then(|ip| self.first_unignored_ipv6(ip, net))
//             .map(IpAddr::V6),
//         _ => panic!("Internal error"),
//     }
//     .ok_or(std::format!("No more unreserved IPs left in {}", net))
// }
