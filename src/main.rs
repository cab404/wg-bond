extern crate base64;
extern crate rand;
extern crate serde;
extern crate serde_json;

use std::fs::{File, read_to_string};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ops::{AddAssign};
use std::str::FromStr;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use crate::wg_tools::{gen_private_key, gen_public_key};

mod wg_tools;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    address: SocketAddr,
    network: IpNetwork,
    private_key: String,
    name: String,
    clients: Vec<ClientConfig>,
    #[serde(default = "Config::f")]
    masquerade: bool,
}

impl Config {
    fn f() -> bool { true }
}


#[derive(Serialize, Deserialize, Debug)]
struct ClientConfig {
    private_key: String,
    name: Option<String>,
    id: u128,
    allowed_ips: Vec<IpNetwork>,
}


fn generate_config() -> Config {
    return Config {
        clients: vec![
            ClientConfig {
                name: Some("some peer".to_string()),
                id: 1,
                private_key: gen_private_key(),
                allowed_ips: vec![
                    IpNetwork::from_str("beef::0/56").unwrap()
                ],
            },
            ClientConfig {
                name: Some("haMephorash".to_string()),
                id: 2,                
                private_key: gen_private_key(),
                allowed_ips: vec![
                    IpNetwork::from_str("192.168.1.0/24").unwrap()
                ],
            },
        ],
        masquerade: true,
        address: SocketAddr::from_str("[beef::1]:63000").unwrap(),
        network: IpNetwork::from_str("beef::1/54").unwrap(),
        private_key: wg_tools::gen_private_key(),
        name: "wgvpn".to_string(),
    };
}

trait WGConfBuilder {
    fn cfg_param<P>(&mut self, name: &str, value: &P) where P: core::fmt::Display;
}

impl WGConfBuilder for String {
    fn cfg_param<P>(&mut self, name: &str, value: &P) where P: core::fmt::Display {
        self.add_assign(name);
        self.add_assign(" = ");
        self.add_assign(value.to_string().as_str());
        self.add_assign("\n");
    }
}

/** Makes an address inside a given network.*/
fn get_network_address_v4(net: &Ipv4Network, num: u32) -> Ipv4Addr {
    assert!(net.size() > num);
    Ipv4Addr::from(u32::from_be_bytes(net.ip().octets().clone()) | (num & (!0u32 >> net.prefix())))
}

fn get_network_address_v6(net: &Ipv6Network, num: u128) -> Ipv6Addr {
    assert!(net.size() > num);
    Ipv6Addr::from(u128::from_be_bytes(net.ip().octets().clone()) | (num & (!0u128 >> net.prefix())))
}

fn get_network_address(net: &IpNetwork, num: u128) -> IpAddr {
    match &net {
        IpNetwork::V4(n) => { IpAddr::V4(get_network_address_v4(&n, num.try_into().unwrap())) }
        IpNetwork::V6(n) => { IpAddr::V6(get_network_address_v6(&n, num.try_into().unwrap())) }
    }
}

fn server_config_toml(config: &Config) -> String {
    let mut built = String::new();
    built.add_assign("[Interface]\n");
    built.cfg_param("Address", &config.address.ip());
    built.cfg_param("ListenPort", &config.address.port());
    built.cfg_param("PrivateKey", &config.private_key);
    for peer in config.clients.iter() {
        match &peer.name {
            Some(a) => {
                built.add_assign("[Peer] # ");
                built.add_assign(a);
                built.add_assign("\n");
            }
            None => {}
        }
        built.cfg_param("PublicKey", &gen_public_key(&peer.private_key));
        let ips = &peer.allowed_ips;
        if !peer.allowed_ips.is_empty() {
            let nets: String = ips.iter()
                .map(IpNetwork::to_string)
                .collect::<Vec<String>>()
                .join(&", ".to_string());
            built.cfg_param("AllowedIPs", &nets)
        }
    }
    built
}

fn client_config_toml(server: &Config, config: &ClientConfig) -> String {
    let mut built = String::new();
    built.add_assign("[Interface]\n");
    built.cfg_param("Address", &get_network_address(&server.network, config.id + 1));
    built.cfg_param("PrivateKey", &config.private_key);
    built.add_assign("[Peer]\n");
    built.cfg_param("AllowedIPs", &server.network);
    built.cfg_param("PublicKey", &gen_public_key(&server.private_key));
    built
}

fn write_config(config: &Config) -> std::io::Result<()> {
    let file = File::create("config.json")?;
    serde_json::to_writer_pretty(file, &config)?;
    Ok(())
}

fn main() {
    
    let config = match read_to_string("config.json") {
        Ok(t) => serde_json::from_str(&t).unwrap(),
        Err(e) => {
            let config = generate_config();
            write_config(&config).unwrap();
            config
        }
    };

    println!("{}", &server_config_toml(&config));
    
    for client in &config.clients {
        println!("{}", &client_config_toml(&config, &client));
    }

}
