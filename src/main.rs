extern crate base64;
extern crate qrcode;
extern crate rand;
extern crate serde;
extern crate serde_json;

use crate::configs::check_endpoint;
use crate::configs::nix::KeyFileExportConfig;
use crate::configs::ConfigType;
use ipnetwork::IpNetwork;
use std::io::Write;
use std::net::IpAddr;
use std::str::FromStr;

use configs::conf::ConfFile;
use configs::nix::NixConf;
use configs::nixops;
use configs::{hosts::export_hosts, qr::QRConfig};

use clap;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

type RVoid = Result<(), String>;

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

fn parse_peer_edit_command(peer: &mut configs::PeerInfo, matches: &clap::ArgMatches) -> RVoid {
    if let Some(endpoint) = matches.value_of("endpoint") {
        peer.endpoint = Some(check_endpoint(endpoint.to_string())?);
    }

    if let Some(dns) = matches.values_of("dns") {
        peer.flags.insert(
            0,
            configs::PeerFlag::DNS {
                addresses: dns
                    .map(IpAddr::from_str)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|f| f.to_string())?,
            },
        )
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

    Ok(())
}

fn command_new_peer(cfg: &mut configs::WireguardNetworkInfo, matches: &clap::ArgMatches) -> RVoid {
    let peer_id = new_id(cfg);
    let name: String = matches.value_of("name").unwrap().into();
    if cfg.by_name(&name).is_some() {
        Err("Peer with that name already exist!")?;
    }

    let mut peer = configs::PeerInfo {
        name,
        endpoint: None,
        id: peer_id,
        private_key: wg_tools::gen_private_key(),
        flags: vec![],
    };

    parse_peer_edit_command(&mut peer, matches)?;

    cfg.peers.append(&mut vec![peer]);

    info!("Peer added!");

    Ok(())
}

fn command_list_peers(cfg: &configs::WireguardNetworkInfo, _: &clap::ArgMatches) -> RVoid {
    // TODO: replace with some table lib
    println!(
        "{peer_name:>12}   {peer_ip:30}   {endpoint:15}",
        peer_name = "Name",
        peer_ip = "IP",
        endpoint = "Endpoint"
    );
    for peer in cfg.peers.iter() {
        let wg_peer = cfg.map_to_interface(peer)?;
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

fn command_edit_peer(cfg: &mut configs::WireguardNetworkInfo, matches: &clap::ArgMatches) -> RVoid {
    let name: String = matches.value_of("name").unwrap().into();
    let mut peer = cfg.by_name_mut(&name).ok_or("No peer with this name.")?;

    parse_peer_edit_command(&mut peer, matches)?;

    Ok(())
}

fn command_export<C: ConfigType>(
    cfg: &configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
    export_options: C::ExportConfig,
) -> RVoid {
    let name: String = matches.value_of("name").unwrap().into();
    let peer = cfg.by_name(&name).ok_or("No peer found with this name.")?;

    let newcfg = &mut cfg.clone();

    if matches.is_present("tunnel") {
        match matches.value_of("tunnel") {
            Some("") => {
                let gateway = cfg
                    .peers
                    .iter()
                    .find(|f| f.has_flag("Gateway"))
                    .ok_or("No gateways found in your config.")?;
                newcfg.peers = vec![gateway.clone(), peer.clone()];
            }
            Some(p) => {
                let gateway = cfg.by_name(p).ok_or("No gateway found by given name")?;
                // if !peer_is_gateway(gateway) {
                //     panic!("Peer with this name is not a gateway!")
                // }
                newcfg.peers = vec![gateway.clone(), peer.clone()];
            }
            None => {}
        };
    };

    println!(
        "{}",
        C::write_config(newcfg.get_configuration(peer)?, export_options)
    );
    Ok(())
}

fn command_export_secrets(
    cfg: &configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
) -> std::io::Result<()> {
    let export_dir = matches.value_of("target").expect("no panik");
    for peer in &cfg.peers {
        std::fs::create_dir_all(format!("{}/{}", export_dir, peer.name))?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!(
                "{}/{}/wg-{}.ed25519.base64",
                export_dir, peer.name, cfg.name
            ))?;
        f.write(peer.private_key.clone().as_bytes())?;
    }
    Ok(())
}

fn edit_params<'a>(subcommand: clap::Command<'a>) -> clap::Command<'a> {
    subcommand
    .arg(clap::Arg::new("endpoint")
            .short('e')
            .long("endpoint")
            .help("Endpoint address of a peer")
            .value_name("ADDRESS:PORT")
            .validator(
                |f| check_endpoint(f.to_string()).map(|_| ()))
            .takes_value(true)
        )
        .arg(clap::Arg::new("dns")
            .short('d')
            .long("dns")
            .help("DNS for a peer")
            .value_name("DNS_1,DNS_2")
            .use_value_delimiter(true)
            .validator(|f| IpAddr::from_str(f)
            .map(|_| ())
            .map_err(|f|f.to_string())
        )
            .takes_value(true)
        )
        .arg(clap::Arg::new("gateway")
            .short('G')
            .long("gateway")
            .help("Whether this peer is a gateway. You may also need -M.")
            .takes_value(false)
        )
        .arg(clap::Arg::new("nixops")
            .short('N')
            .long("nixops")
            .help("Whether this peer is a NixOps machine, and should be added to a NixOps export.")
            .takes_value(false)
        )
        .arg(clap::Arg::new("center")
            .short('C')
            .long("center")
            .help("Whether this peer is to be used as connection point for other peers.")
            .takes_value(false)
        )
        .arg(clap::Arg::new("masquerade")
            .short('M')
            .long("masquerade")
            .help("Whether to enable iptables masquerade on this peer.")
            .takes_value(true)
            .value_name("INTERFACE")
        )
        .arg(clap::Arg::new("keepalive")
            .short('K')
            .long("keepalive")
            .help("Keepalive interval of a host")
            .validator(|v|
                match u16::from_str(v) {
                    Ok(_) => Ok(()),
                    Err(_) => Err("Not a number.".to_string()),
                }
            )
            .takes_value(true)
            .value_name("SECONDS")
        )
}

fn export_params<'a>(subcommand: clap::Command<'a>) -> clap::Command<'a> {
    subcommand
        .arg(
            clap::Arg::new("name")
                .help("Name of a new peer")
                .required(true),
        )
        .arg(
            clap::Arg::new("tunnel")
                .short('T')
                .help("Whether to remove all peers from resulting config except a gateway")
                .takes_value(true)
                .value_name("GATEWAY NAME"),
        )
}

fn main() {
    pretty_env_logger::init();
    // std::panic::set_hook(Box::new(panic_hook));

    let args = clap::Command::new("wg-bond")
        .version("0.3.0")
        .about("Wireguard configuration manager")
        .author("Vladimir Serov <cab404>")
        .long_about("Wireguard configuration manager.\nSources: https://gitlab.com/cab404/wg-bond.")
        .subcommand_required(true)
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .help("Config file to use")
                .value_name("FILE")
                .default_value("./wg-bond.json")
                .takes_value(true)
                .use_value_delimiter(false),
        )
        .subcommand(
            clap::Command::new("init")
                .about("Initializes a config file")
                .arg(clap::Arg::new("name").help("Network name").required(true))
                .arg(
                    clap::Arg::new("network")
                        .short('n')
                        .long("network")
                        .help("Network for peers to use")
                        .value_name("IP/MASK")
                        .validator(|f| {
                            IpNetwork::from_str(f)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        })
                        .default_value("10.0.0.0/24")
                        .use_value_delimiter(false)
                        .takes_value(true),
                ),
        )
        .subcommand(
            edit_params(clap::Command::new("add"))
                .about("Adds a new peer to the network")
                .arg(
                    clap::Arg::new("name")
                        .help("Name for a new peer")
                        .required(true),
                ),
        )
        .subcommand(clap::Command::new("list").about("Lists all added peers"))
        .subcommand(
            edit_params(clap::Command::new("edit"))
                .about("Edits existing peer")
                .arg(
                    clap::Arg::new("name")
                        .help("Name of a new peer")
                        .required(true),
                ),
        )
        .subcommand(
            export_params(clap::Command::new("nix"))
                .arg(
                    clap::Arg::new("separate-secrets")
                    .long("separate-secrets")
                    .takes_value(false)
                    .help(
                        "Whether to use external secrets, to avoid putting secrets in the store",
                    ),
                )
                .about("Generates Nix configs"),
        )
        .subcommand(clap::Command::new("nixops").about("Generates NixOps config for all peers"))
        .subcommand(
            clap::Command::new("secrets")
                .about("Generates secret files for all peers")
                .arg(
                    clap::Arg::new("target")
                        .help("Where to export the secrets")
                        .default_value("./secrets"),
                ),
        )
        .subcommand(clap::Command::new("hosts").about("Generates /etc/hosts for all peers"))
        .subcommand(
            clap::Command::new("rm").about("Deletes a peer").arg(
                clap::Arg::new("name")
                    .help("Name of a new peer")
                    .required(true),
            ),
        )
        .subcommand(export_params(clap::Command::new("qr")).about("Generates QR code with config"))
        .subcommand(export_params(clap::Command::new("conf")).about("Generates wg-quick configs"))
        .get_matches();

    let cfg_file = args.value_of("config").unwrap();

    let mut net = if let Some(matches) = args.subcommand_matches("init") {
        command_init_config(matches)
    } else {
        read_config(cfg_file).unwrap()
    };

    fn command_remove(
        cfg: &mut configs::WireguardNetworkInfo,
        matches: &clap::ArgMatches,
    ) -> RVoid {
        let name = matches.value_of("name").ok_or("".to_string())?;
        let peer = cfg
            .peers
            .iter()
            .position(|f| f.name == name)
            .ok_or("".to_string())?;
        cfg.peers.remove(peer);
        Ok(())
    }

    fn commands(net: &mut configs::WireguardNetworkInfo, args: &clap::ArgMatches) -> RVoid {
        match args.subcommand() {
            Some(("add", matches)) => command_new_peer(net, matches),
            Some(("list", matches)) => command_list_peers(net, matches),
            Some(("edit", matches)) => command_edit_peer(net, matches),
            Some(("nix", matches)) => {
                let conf = configs::nix::NixExportConfig {
                    use_keyfile: if matches.is_present("separate-secrets") {
                        Some(KeyFileExportConfig {
                            target_prefix: "/secrets".into(),
                        })
                    } else {
                        None
                    },
                };
                command_export::<NixConf>(net, matches, conf)
            }
            Some(("conf", matches)) => command_export::<ConfFile>(net, matches, ()),
            Some(("qr", matches)) => command_export::<QRConfig>(net, matches, ()),
            Some(("rm", matches)) => command_remove(net, matches),
            Some(("hosts", _)) => {
                println!("{}", export_hosts(net)?);
                Ok(())
            }
            Some(("secrets", matches)) => {
                command_export_secrets(net, matches).map_err(|e| e.to_string())
            }
            Some(("nixops", _)) => {
                println!(
                    "{}",
                    nixops::write_config(
                        net,
                        configs::nix::NixExportConfig {
                            use_keyfile: Some(KeyFileExportConfig {
                                target_prefix: "/secrets".into()
                            })
                        }
                    )?
                );
                Ok(())
            }
            _ => Ok(()),
        }
    }

    match commands(&mut net, &args) {
        Ok(()) => {
            save_config(&net, cfg_file).unwrap();
        }
        Err(e) => println!("{}", e),
    }
}
