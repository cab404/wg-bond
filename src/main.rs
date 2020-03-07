extern crate base64;
extern crate rand;
extern crate serde;
extern crate serde_json;
extern crate qrcode;

use std::str::FromStr;
use ipnetwork::{IpNetwork};
use configs::conf::ConfFile;
use configs::nix::NixConf;
use configs::qr::QRConfig;

use clap;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

mod wg_tools;
mod configs;
use std::iter::*;

// type editablecfg: &mut configs::WireguardNetworkInfo

fn default_config() -> configs::WireguardNetworkInfo {
    configs::WireguardNetworkInfo {
        name: "wgvpn".to_string(),
        network: IpNetwork::from_str("10.0.0.0/24").unwrap(),
        peers: vec![]
    }
}

fn read_config(fname: &str) -> configs::WireguardNetworkInfo {
    debug!("Opening config from {}", fname);

    match std::fs::OpenOptions::new().create(false).read(true).open(fname) {
        Ok(handle) => {
            let result : Result<configs::WireguardNetworkInfo, serde_json::Error> = serde_json::from_reader(std::io::BufReader::new(handle));
            match result {
                Ok(net) => {
                    net
                }
                Err(serde_err) => {
                    warn!("Cannot deserialize config file, {}", serde_err);
                    default_config()
                }
            }
        }
        Err(fs_err) => {
            warn!("Cannot open config file, {}", fs_err);
            default_config()
        }
    }

}

fn write_config(cfg: &configs::WireguardNetworkInfo, fname: &str) -> Result<(), std::io::Error> {
    std::fs::OpenOptions::new()
        .create(true).write(true).open(fname)
        .map(|f| std::io::BufWriter::new(f) )
        .and_then(|writer| Ok(serde_json::to_writer_pretty(writer, cfg).unwrap()))
}

fn new_id(cfg: &configs::WireguardNetworkInfo) -> u128 {
    cfg.peers.iter().map(|i| i.id).max().unwrap_or(0) + 1
}

fn new_peer(cfg: &mut configs::WireguardNetworkInfo, matches: &clap::ArgMatches) {    
    let peer_id = new_id(cfg);
    let name: String = matches.value_of("name").unwrap().into();

    cfg.peers.append(&mut vec![configs::PeerInfo {
        name: Some(name),
        endpoint: None,
        id: peer_id,
        private_key: wg_tools::gen_private_key(),
        flags: vec![]
    }]);
    info!("Peer with id {id} added!", id = peer_id);    
}

fn main() {
    pretty_env_logger::init();

    let args = clap::App::new("wgbond")
        .version("0.1")
        .about("Wireguard configuration manager")
        .author("Vladimir Serov <cab404>")
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Config file to use")
                .value_name("FILE")
                .default_value("config.json")
                .takes_value(true)
                .use_delimiter(false)
        )
        .subcommand(
            clap::SubCommand::with_name("add")
                .about("Adds a new peer to the network")
                .arg(clap::Arg::with_name("name")
                    .help("Name for a new peer")
                    .required(true)
                )

        )
        .subcommand(
            clap::SubCommand::with_name("nix")
                .about("Generates Nix configs")
                .arg(clap::Arg::with_name("id")
                    .help("Id of a peer")
                )
        )
        .get_matches();
    
    let cfg_file = args.value_of("config").unwrap();
    let mut net = read_config(cfg_file);

    if let Some(matches) = args.subcommand_matches("add") {
        new_peer(&mut net, matches);
    }

    write_config(&net, cfg_file).unwrap();

    // let mut net = configs::WireguardNetworkInfo {
    //     name: "wgvpn".to_string(),
    //     network: IpNetwork::from_str("10.0.0.0/24").unwrap(),
    //     peers: vec![
    //         configs::PeerInfo {
    //             name: Some("One peer".to_string()), 
    //             endpoint: Some(SocketAddr::from_str("73.2.1.3:64000").unwrap()),
    //             id: 88,
    //             private_key: wg_tools::gen_private_key(),
    //             flags: vec![
    //                 configs::PeerFlag::Masquerade { interface: "eth0".to_string() },
    //                 configs::PeerFlag::Gateway { ignore_local_networks: true },
    //             ]
    //         },
    //         configs::PeerInfo {
    //             name: Some("Another peer".to_string()), 
    //             endpoint: None,
    //             id: 89,
    //             private_key: wg_tools::gen_private_key(),
    //             flags: vec![
    //                 configs::PeerFlag::Keepalive { keepalive: 32 },
    //             ]
    //         }
    //     ]
    // };
    // new_peer(&mut net, "test".into());

    // println!("{}", serde_json::to_string_pretty(&net).unwrap());

    // for a in net.peers.iter() {
    //     println!("{}", ConfFile::write_config(&net, a.id));
    // }

}

