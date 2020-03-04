extern crate base64;
extern crate rand;
extern crate serde;
extern crate serde_json;
extern crate qrcode;

use crate::configs::ConfigType;
use std::fs::{File, read_to_string};
use std::str::FromStr;
use ipnetwork::{IpNetwork};
use std::net::{SocketAddr, IpAddr};
use crate::configs::{Interface};

use configs::conf::ConfFile;
use configs::nix::NixConf;
use configs::qr::QRConfig;
use qrcode::QrCode;
use qrcode::render::unicode;

mod wg_tools;
mod configs;

fn main() {

    let net = configs::WireguardNetworkInfo {
        name: "wgvpn".to_string(),
        network: IpNetwork::from_str("10.0.0.0/24").unwrap(),
        peers: vec![
            configs::PeerInfo {
                name: Some("One peer".to_string()), 
                addresses: vec![ IpAddr::from_str("73.2.1.3").unwrap()  ],
                id: 88,
                port: Some(64000),
                private_key: wg_tools::gen_private_key(),
                flags: vec![
                    configs::PeerFlag::Masquerade { interface: "eth0".to_string() },
                    configs::PeerFlag::Gateway { ignore_local_networks: true },
                ]
            },
            configs::PeerInfo {
                name: Some("Another peer".to_string()), 
                addresses: vec![],
                id: 89,
                port: Some(64000),
                private_key: wg_tools::gen_private_key(),
                flags: vec![
                    configs::PeerFlag::Keepalive { keepalive: 32 },
                ]
            }
        ]
    };
    println!("{}", NixConf::write_config(&net, 88));
    println!("{}", NixConf::write_config(&net, 89));
    // println!("{}", QRConfig::write_config(&net, 89));
    println!("{}", ConfFile::write_config(&net, 89));

}

