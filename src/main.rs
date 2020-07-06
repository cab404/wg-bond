extern crate base64;
extern crate qrcode;
extern crate rand;
extern crate serde;
extern crate serde_json;

use crate::configs::check_endpoint;
use crate::configs::{ConfigType, ConfigWriter};
use clap::AppSettings::SubcommandRequired;
use ipnetwork::IpNetwork;
use std::str::FromStr;

use configs::conf::ConfFile;
use configs::nix::NixConf;
use configs::nixops::NixOpsConf;
use configs::qr::QRConfig;

use clap;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

mod configs;
mod wg_tools;
use std::iter::*;

fn read_config(fname: &str) -> Result<configs::WireguardNetworkInfo, String> {
    debug!("Opening config from {}", fname);

    match std::fs::OpenOptions::new()
        .create(false)
        .read(true)
        .open(fname)
    {
        Ok(handle) => {
            let result: Result<configs::WireguardNetworkInfo, serde_json::Error> =
                serde_json::from_reader(std::io::BufReader::new(handle));
            match result {
                Ok(net) => Ok(net),
                Err(serde_err) => Err(format!("Cannot deserialize config file, {}", serde_err)),
            }
        }
        Err(fs_err) => Err(format!("Cannot open config file, {}", fs_err)),
    }
}

fn save_config(cfg: &configs::WireguardNetworkInfo, fname: &str) -> Result<(), std::io::Error> {
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(fname)
        .map(std::io::BufWriter::new)
        .map(|writer| serde_json::to_writer_pretty(writer, cfg).unwrap())
}

fn new_id(cfg: &configs::WireguardNetworkInfo) -> u128 {
    cfg.peers.iter().map(|i| i.id).max().unwrap_or(0) + 1
}

fn command_init_config(matches: &clap::ArgMatches) -> configs::WireguardNetworkInfo {
    let name: &str = matches.value_of("name").unwrap();
    let net: &str = matches.value_of("network").unwrap();

    configs::WireguardNetworkInfo {
        name: name.to_string(),
        networks: vec![IpNetwork::from_str(net).unwrap()],
        flags: vec![],
        peers: vec![],
    }
}

fn parse_peer_edit_command(peer: &mut configs::PeerInfo, matches: &clap::ArgMatches) {
    if let Some(endpoint) = matches.value_of("endpoint") {
        peer.endpoint = check_endpoint(endpoint).map(str::to_string);
    }

    if let Some(interface) = matches.value_of("masquerade") {
        peer.flags.insert(
            0,
            configs::PeerFlag::Masquerade {
                interface: interface.into(),
            },
        )
    }

    if matches.is_present("center") {
        peer.flags.insert(0, configs::PeerFlag::Center)
    }

    if matches.is_present("gateway") {
        peer.flags.insert(
            0,
            configs::PeerFlag::Gateway {
                ignore_local_networks: true,
            },
        )
    }

    if matches.is_present("nixops") {
        peer.flags.insert(0, configs::PeerFlag::NixOpsMachine)
    }

    if let Some(keepalive) = matches
        .value_of("keepalive")
        .map(|n| u16::from_str(n).unwrap())
    {
        peer.flags
            .insert(0, configs::PeerFlag::Keepalive { keepalive })
    }

    peer.flags.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    peer.flags.dedup_by(|a, b| a.as_ref() == b.as_ref());
}

fn command_new_peer(
    cfg: &mut configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
) -> Result<(), u8> {
    let peer_id = new_id(cfg);
    let name: String = matches.value_of("name").unwrap().into();
    if cfg.by_name(&name).is_some() {
        println!("Peer with that name already exist!");
        return Err(1);
    }

    let mut peer = configs::PeerInfo {
        name,
        endpoint: None,
        id: peer_id,
        private_key: wg_tools::gen_private_key(),
        flags: vec![],
    };

    parse_peer_edit_command(&mut peer, matches);

    cfg.peers.append(&mut vec![peer]);

    info!("Peer added!");

    Ok(())
}

fn command_list_peers(cfg: &configs::WireguardNetworkInfo, _: &clap::ArgMatches) -> Result<(), u8> {
    // TODO: replace with some table lib
    println!(
        "{peer_name:>12}   {peer_ip:30}   {endpoint:15}",
        peer_name = "Name",
        peer_ip = "IP",
        endpoint = "Endpoint"
    );
    for peer in cfg.peers.iter() {
        let wg_peer = cfg.map_to_interface(peer);
        println!(
            "{name:>12}   {ip:30}   {endpoint:15}",
            name = peer.name,
            ip = wg_peer
                .address
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", "), // if it doesn't unwrap, something is really bad on our side
            endpoint = peer.endpoint.clone().unwrap_or_else(|| "".into())
        );
    }
    Ok(())
}

fn command_edit_peer(
    cfg: &mut configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
) -> Result<(), u8> {
    let name: String = matches.value_of("name").unwrap().into();
    let mut peer = cfg.by_name_mut(&name).expect("No peer with this name.");

    parse_peer_edit_command(&mut peer, matches);

    Ok(())
}

fn command_export(
    cfg: &configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
    exporter: ConfigWriter,
) -> Result<(), u8> {
    let name: String = matches.value_of("name").unwrap().into();
    let peer = cfg.by_name(&name).expect("No peer found with this name.");

    let newcfg = &mut cfg.clone();

    if matches.is_present("tunnel") {
        match matches.value_of("tunnel") {
            Some("") => {
                let gateway = cfg
                    .peers
                    .iter()
                    .find(|f| f.has_flag("Gateway"))
                    .expect("No gateways found in your config.");
                newcfg.peers = vec![gateway.clone(), peer.clone()];
            }
            Some(p) => {
                let gateway = cfg.by_name(p).expect("No gateway found by given name");
                // if !peer_is_gateway(gateway) {
                //     panic!("Peer with this name is not a gateway!")
                // }
                newcfg.peers = vec![gateway.clone(), peer.clone()];
            }
            None => {}
        };
    };

    println!("{}", exporter(&newcfg, peer.id));
    Ok(())
}

fn edit_params<'a, 'b>(subcommand: clap::App<'a, 'b>) -> clap::App<'a, 'b> {
    subcommand
        .arg(clap::Arg::with_name("endpoint")
            .short("e")
            .long("endpoint")
            .help("Endpoint address of a peer")
            .value_name("ADDRESS:PORT")
            .use_delimiter(false)
            .takes_value(true)
        )
        .arg(clap::Arg::with_name("gateway")
            .short("G")
            .long("gateway")
            .help("Whether this peer is a gateway. You may also need -M.")
            .use_delimiter(false)
            .takes_value(false)
        )
        .arg(clap::Arg::with_name("nixops")
            .short("N")
            .long("nixops")
            .help("Whether this peer is a NixOps machine, and should be added to a NixOps export.")
            .use_delimiter(false)
            .takes_value(false)
        )
        .arg(clap::Arg::with_name("center")
            .short("C")
            .long("center")
            .help("Whether this peer is to be used as connection point for other peers.")
            .use_delimiter(false)
            .takes_value(false)
        )
        .arg(clap::Arg::with_name("masquerade")
            .short("M")
            .long("masquerade")
            .help("Whether to enable iptables masquerade on this peer.")
            .use_delimiter(false)
            .takes_value(true)
            .value_name("INTERFACE")
        )
        .arg(clap::Arg::with_name("keepalive")
            .short("K")
            .long("keepalive")
            .help("Keepalive interval of a host")
            .validator(|v|
                match u16::from_str(v.as_str()) {
                    Ok(_) => Ok(()),
                    Err(_) => Err("Not a number.".to_string()),
                }
            )
            .use_delimiter(false)
            .takes_value(true)
            .value_name("SECONDS")
        )
}

fn export_params<'a, 'b>(subcommand: clap::App<'a, 'b>) -> clap::App<'a, 'b> {
    subcommand
        .arg(
            clap::Arg::with_name("name")
                .help("Name of a new peer")
                .required(true),
        )
        .arg(
            clap::Arg::with_name("tunnel")
                .short("T")
                .help("Whether to remove all peers from resulting config except a gateway")
                .use_delimiter(false)
                .takes_value(true)
                .value_name("GATEWAY NAME"),
        )
}

fn main() {
    pretty_env_logger::init();
    // std::panic::set_hook(Box::new(panic_hook));

    let args = clap::App::new("wgbond")
        .version("0.1")
        .about("Wireguard configuration manager")
        .author("Vladimir Serov <cab404>")
        .setting(SubcommandRequired)
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Config file to use")
                .value_name("FILE")
                .default_value("wg-bond.json")
                .takes_value(true)
                .use_delimiter(false),
        )
        .subcommand(
            clap::SubCommand::with_name("init")
                .about("Initializes a config file")
                .arg(
                    clap::Arg::with_name("name")
                        .help("Network name")
                        .required(true),
                )
                .arg(
                    clap::Arg::with_name("network")
                        .short("n")
                        .long("network")
                        .help("Network for peers to use")
                        .value_name("IP/MASK")
                        .default_value("10.0.0.0/24")
                        .use_delimiter(false)
                        .takes_value(true),
                ),
        )
        .subcommand(
            edit_params(clap::SubCommand::with_name("add"))
                .about("Adds a new peer to the network")
                .arg(
                    clap::Arg::with_name("name")
                        .help("Name for a new peer")
                        .required(true),
                ),
        )
        .subcommand(clap::SubCommand::with_name("list").about("Lists all added peers"))
        .subcommand(
            edit_params(clap::SubCommand::with_name("edit"))
                .about("Edits existing peer")
                .arg(
                    clap::Arg::with_name("name")
                        .help("Name of a new peer")
                        .required(true),
                ),
        )
        .subcommand(
            export_params(clap::SubCommand::with_name("nix")).about("Generates Nix configs"),
        )
        .subcommand(
            clap::SubCommand::with_name("nixops").about("Generates NixOps config for all peers"),
        )
        .subcommand(
            export_params(clap::SubCommand::with_name("qr")).about("Generates QR code with config"),
        )
        .subcommand(
            export_params(clap::SubCommand::with_name("conf")).about("Generates wg-quick configs"),
        )
        .get_matches();

    let cfg_file = args.value_of("config").unwrap();

    let mut net = if let Some(matches) = args.subcommand_matches("init") {
        command_init_config(matches)
    } else {
        read_config(cfg_file).unwrap()
    };

    fn commands(
        net: &mut configs::WireguardNetworkInfo,
        args: &clap::ArgMatches,
    ) -> Result<(), u8> {
        match args.subcommand() {
            ("add", Some(matches)) => command_new_peer(net, matches),
            ("list", Some(matches)) => command_list_peers(net, matches),
            ("edit", Some(matches)) => command_edit_peer(net, matches),
            ("nix", Some(matches)) => command_export(net, matches, NixConf::write_config),
            ("conf", Some(matches)) => command_export(net, matches, ConfFile::write_config),
            ("qr", Some(matches)) => command_export(net, matches, QRConfig::write_config),
            ("nixops", Some(_)) => {
                println!("{}", NixOpsConf::write_config(net, 0));
                Ok(())
            }
            _ => Ok(()),
        }
    }

    match commands(&mut net, &args) {
        Ok(()) => {
            save_config(&net, cfg_file).unwrap();
        }
        Err(e) => println!("Error occured ({})", e),
    }
}

// fn panic_hook(info: &std::panic::PanicInfo<'_>) {
//     println!("We panicked.");
//     println!("mowmow : {:?}", info.payload());
// }
