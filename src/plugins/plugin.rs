use std::{collections::HashMap, process::Output};

use clap::Result;
use serde::{Deserialize, Serialize};
use serde_json::Error;

use crate::configs::{
    ConfigType, Interface, NetworkFlag, Peer, PeerFlag, WireguardConfiguration,
    WireguardNetworkInfo,
};

type WgC = WireguardConfiguration;
type WgNI = WireguardNetworkInfo;

/// Modifies behavior of network generation
pub trait NetworkPlugin {
    fn apply_to_network(&self, network: &WgNI) -> WgNI;
}

/// Modifies behavior of peer generation
pub trait PeerPlugin {
    type Config;

    fn new(config: Self::Config);

    fn apply_to_configuration(&self, network: &WgNI, configuration: WgC) -> WgC {
        configuration
    }
    fn apply_to_interface(&self, network: &WgNI, interface: Interface) -> Interface {
        interface
    }
    fn apply_to_peer(&self, network: &WgNI, peer: Peer) -> Peer {
        peer
    }
}

// macro_rules! fct {
//     (@step $s:literal ($drop:literal, $($smth:literal),*)) => {
//         (fct!(@mult ($($smth),*) ($($smth),*)))
//     };
//     (@mult ($($e:literal),*) $rec:tt) => {
//         $(fct!(@step $e $rec) +) + fct!(@step 0 $rec)
//     };
//     (@step $s:literal ($one:literal)) => { 1 };
//     (@step $s:literal ()) => { 1 };
//     ($($a:literal),*) => {
//         fct!(@step 0 ($($a),*))
//     };
// }
// #[test]
// fn fct() {
//     let n = fct!(1,1,1,1,1,1,1,1);
//     assert_eq!(n, 5040);
// }

// macro_rules! compose_plugins {
//     (@internal $comb:tt ; $({$name:expr, $plugin:ident}) +)  => {
//         $(
//             let $plugin = $name;
//             format!("{}", $comb);
//         )*
//     };
//     ($($jolly:expr),+; $({$name:expr, $plugin:ident}) +) => {
//         compose_plugins!( @internal ($(
//             format!("{:?}", $jolly)
//         ),+); $({$name, $plugin})+ );
//     };
// }

// #[test]
// fn something() {
//     // let f = (mfmacro!(1,2,3,4,5,6,7,8,9));
//     compose_plugins! {
//         12,12,"15 avocados",12;
//         {1, mov}
//         {3, wow}
//         {3, cow}
//     };
// }

// pub trait RegEntry<Config, Output> {
//     fn generate(self, cfg: Config) -> Result<Output, Error>;
// }

// pub struct GeneratorMap<Config, Output> {
//     plugins: HashMap<String, Box<dyn RegEntry<Config, Output>>>,
// }

// impl<C, O> GeneratorMap<C, O> {
//     fn new() -> Self {
//         GeneratorMap {
//             plugins: HashMap::new(),
//         }
//     }

//     fn get_value(&self, key: String, cfg: C) -> Result<O, Error> {
//         let f = self.plugins.get(&key).unwrap().clone();
//         f.generate(cfg).unwrap();
//         panic!("");
//     }
// }

// #[test]
// fn testme() {
//     struct TReg;
//     impl PeerPlugin for &TReg {
//         fn apply_to_configuration(&self, network: &WgNI, configuration: WgC) -> WgC {
//             configuration
//         }

//         fn apply_to_peer(&self, network: &WgNI, peer: Peer) -> Peer {
//             peer
//         }

//         fn apply_to_interface(&self, network: &WgNI, interface: Interface) -> Interface {
//             interface
//         }
//     }
//     impl<'a> RegEntry<(), Box<dyn PeerPlugin + 'a>> for TReg {
//         fn generate(self: TReg, _: ()) -> Result<Box<dyn PeerPlugin + 'a>, Error> {
//             self.generate(())
//         }
//     }

//     let mut registry = GeneratorMap::new();
//     registry
//         .plugins
//         .insert("rest".to_string(), Box::new(TReg {}));

//     let r = registry
//         .get_value("rest".to_string(), ())
//         .unwrap()
//         .apply_to_peer(
//             &WireguardNetworkInfo {
//                 name: "".to_string(),
//                 flags: vec![],
//                 networks: vec![],
//                 peers: vec![],
//             },
//             Peer {
//                 public_key: "".to_string(),
//                 preshared_key: None,
//                 allowed_ips: vec![],
//                 endpoint: None,
//                 persistent_keepalive: None,
//             },
//         );

//     assert_eq!(r.endpoint, None)
// }
