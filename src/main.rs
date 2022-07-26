extern crate base64;
extern crate qrcode;
extern crate rand;
extern crate serde;
extern crate serde_json;

use crate::configs::nix::KeyFileExportConfig;
use crate::configs::ConfigType;
use crate::configs::{check_endpoint, IpNetDifference};
use ipnetwork::IpNetwork;
use std::collections::HashSet;
use std::io::Write;
use std::net::IpAddr;
use std::str::FromStr;

use configs::conf::ConfFile;
use configs::nix::NixConf;
use configs::{hosts::export_hosts, qr::QRConfig};
use configs::{nixops, PeerFlag, PeerInfo, WireguardNetworkInfo};

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
    let net = IpNetwork::from_str(matches.value_of("network").unwrap()).unwrap();
    configs::WireguardNetworkInfo {
        name: name.to_string(),
        networks: vec![net],
        flags: vec![],
        peers: vec![],
        ignored_ipv4: HashSet::new(),
        ignored_ipv6: HashSet::new(),
    }
}

fn parse_peer_edit_command(
    cfg: &WireguardNetworkInfo,
    peer: &mut configs::PeerInfo,
    matches: &clap::ArgMatches,
) -> RVoid {
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

    if matches.is_present("is-template") {
        peer.flags.insert(0, configs::PeerFlag::Template);
    }

    if let Some(template_name) = matches.value_of("use-template") {
        if let Some(template) = cfg.by_name(template_name) {
            peer.flags
                .insert(0, configs::PeerFlag::UseTemplate { peer: template.id });
        } else {
            Err(format!(
                "Peer you are trying to use as a template ({}) doesn't exist!",
                template_name
            ))?
        }
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
        ips: vec![],
    };

    parse_peer_edit_command(cfg, &mut peer, matches)?;

    for net in &cfg.networks {
        peer.ips.push(cfg.get_free_net_address(*net)?);
    }
    cfg.peers.push(peer);
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
    for peer in cfg.real_peers() {
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

    let cfg_copy = cfg.clone();
    let peer = cfg.by_name_mut(&name).ok_or("No peer with this name.")?;

    parse_peer_edit_command(&cfg_copy, peer, matches)?;

    Ok(())
}

fn command_export<C: ConfigType>(
    cfg: &configs::WireguardNetworkInfo,
    matches: &clap::ArgMatches,
    export_options: C::ExportConfig,
) -> RVoid {
    let name: String = matches.value_of("name").unwrap().into();

    let peer = cfg.by_name(&name).ok_or("No peers found with this name.")?;

    if peer.is_template() {
        Err(format!(
            "Peer you are trying to export ({}) is a template!",
            name
        ))?
    }

    let newcfg = &mut cfg.clone();

    if matches.is_present("tunnel") {
        match matches.value_of("tunnel") {
            Some("") => {
                let gateway = cfg
                    .real_peers()
                    .iter()
                    .find(|f| f.has_flag("Gateway"))
                    .cloned()
                    .ok_or("No gateways found in your config.")?;
                newcfg.peers = vec![gateway.clone(), peer.clone()];
            }
            Some(p) => {
                // Should we search in templates???
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
    for peer in &cfg.real_peers() {
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
        .arg(clap::Arg::new("is-template")
            .long("is-template")
            .help("Whether this peer is a template to base other peers on.")
            .takes_value(false)
        )
        .arg(clap::Arg::new("use-template")
            .short('T')
            .long("use-template")
            .help("Specifies on which other peer to base this upon")
            .takes_value(true)
            .value_name("PEER NAME")
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

fn ipnetwork_validator(s: &str) -> RVoid {
    IpNetwork::from_str(s)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn main_app<'a>() -> clap::Command<'a> {
    clap::Command::new("wg-bond")
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
                    .validator(ipnetwork_validator)
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
                    .help("Whether to use external secrets, to avoid putting secrets in the store"),
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
    .subcommand(
        clap::Command::new("ignore")
            .about("Sets IP range to ignore when generating IPs for peers")
            .arg(
                clap::Arg::new("range")
                    .help("IP range to ignore")
                    .required(true)
                    .value_name("IP/MASK")
                    .validator(ipnetwork_validator),
            ),
    )
    .subcommand(
        clap::Command::new("unignore")
        .about("Allows generating peer IPs in specified IP range. Overrides overlapping 'ignored' ranges.")
        .arg(
            clap::Arg::new("range")
            .help("IP range to unignore")
            .required(true)
            .value_name("IP/MASK")
            .validator(ipnetwork_validator),
        ),
    )
}

fn main() {
    pretty_env_logger::init();
    // std::panic::set_hook(Box::new(panic_hook));

    let args = main_app().get_matches();

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

    // panics if prefix of ignored network is 0 because of this issue:
    // https://github.com/achanda/ipnetwork/issues/130
    fn command_ignore_range(
        cfg: &mut configs::WireguardNetworkInfo,
        matches: &clap::ArgMatches,
    ) -> RVoid {
        let s = matches.value_of("range").ok_or("".to_string())?;
        let range = IpNetwork::from_str(s).map_err(|f| f.to_string())?;
        ignore_range(cfg, range)
    }

    fn command_unignore(
        cfg: &mut configs::WireguardNetworkInfo,
        matches: &clap::ArgMatches,
    ) -> RVoid {
        let s = matches.value_of("range").ok_or("".to_string())?;
        let range = IpNetwork::from_str(s).map_err(|f| f.to_string())?;
        unignore_range(cfg, range)
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
            Some(("ignore", matches)) => command_ignore_range(net, matches),
            Some(("unignore", matches)) => command_unignore(net, matches),
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

fn ignore_range_common<T, Sup: Fn(&T, &T) -> bool, Sub: Fn(&T, &T) -> bool>(
    ignored: &mut HashSet<T>,
    to_ignore: T,
    is_supernet: Sup,
    is_subnet: Sub,
) where
    T: std::fmt::Display + Clone + std::cmp::Eq + std::hash::Hash,
{
    let subranges = ignored
        .iter()
        .filter(|inner| is_subnet(inner, &to_ignore))
        .cloned()
        .collect::<Vec<_>>();

    for inner in subranges {
        println!(
            "An already ignored subnet {} will now be covered by {}",
            inner, to_ignore
        );
        ignored.remove(&inner);
    }
    if let Some(ex) = ignored.iter().find(|outer| is_supernet(outer, &to_ignore)) {
        println!("A supernet {} covering given net is already ignored", ex)
    } else {
        ignored.insert(to_ignore);
    }
}

fn ignore_range(cfg: &mut configs::WireguardNetworkInfo, range: IpNetwork) -> RVoid {
    let contains = |ip: &IpAddr| match (*ip, range) {
        (IpAddr::V4(ip), IpNetwork::V4(range)) => range.contains(ip),
        (IpAddr::V6(ip), IpNetwork::V6(range)) => range.contains(ip),
        _ => false,
    };
    if let Some(_) = cfg.assigned_ips().into_iter().find(contains) {
        return Err("Aborting: there are assigned IPs in specified range.".to_string());
    }

    match range {
        IpNetwork::V4(range) => ignore_range_common(
            &mut cfg.ignored_ipv4,
            range,
            |a, b| a.is_supernet_of(*b),
            |a, b| a.is_subnet_of(*b),
        ),
        IpNetwork::V6(range) => ignore_range_common(
            &mut cfg.ignored_ipv6,
            range,
            |a, b| a.is_supernet_of(*b),
            |a, b| a.is_subnet_of(*b),
        ),
    }
    Ok(())
}

fn unignore_range(cfg: &mut WireguardNetworkInfo, range: IpNetwork) -> RVoid {
    match range {
        IpNetwork::V4(range) => {
            let rem = IpNetDifference::subtract_all(&cfg.ignored_ipv4, &range);
            cfg.ignored_ipv4.clear();
            cfg.ignored_ipv4.extend(rem);
            Ok(())
        }
        IpNetwork::V6(range) => {
            let rem = IpNetDifference::subtract_all(&cfg.ignored_ipv6, &range);
            cfg.ignored_ipv6.clear();
            cfg.ignored_ipv6.extend(rem);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use ipnetwork::Ipv4Network;

    use crate::configs::WireguardNetworkInfo;

    use super::*;

    fn new_config(ip_addr: &str) -> WireguardNetworkInfo {
        let matches = main_app().get_matches_from(["wg-bond", "init", "testnet", "-n", ip_addr]);
        let sub_matches = matches.subcommand_matches("init").unwrap();
        command_init_config(&sub_matches)
    }

    fn add_peer(cfg: &mut WireguardNetworkInfo, name: &str) -> RVoid {
        command_new_peer(
            cfg,
            main_app()
                .get_matches_from(["wg-bond", "add", name])
                .subcommand_matches("add")
                .unwrap(),
        )
    }

    #[test]
    fn test_ignore_ipv4() {
        let mut cfg = new_config("10.0.0.0/24");

        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.1/32").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.3/32").unwrap()).unwrap();

        add_peer(&mut cfg, "1").unwrap();
        add_peer(&mut cfg, "2").unwrap();
        let peer1 = cfg.peers.iter().find(|p| p.name == "1").unwrap();
        let peer2 = cfg.peers.iter().find(|p| p.name == "2").unwrap();

        assert_eq!(peer1.ips, vec![IpAddr::from_str("10.0.0.2").unwrap()]);
        assert_eq!(peer2.ips, vec![IpAddr::from_str("10.0.0.4").unwrap()]);
    }

    #[test]
    fn test_ignore_ipv6() {
        let mut cfg = new_config("10::0/96");

        ignore_range(&mut cfg, IpNetwork::from_str("10::0/127").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10::4/127").unwrap()).unwrap();

        add_peer(&mut cfg, "1").unwrap();
        add_peer(&mut cfg, "2").unwrap();
        add_peer(&mut cfg, "3").unwrap();
        let peer1 = cfg.peers.iter().find(|p| p.name == "1").unwrap();
        let peer2 = cfg.peers.iter().find(|p| p.name == "2").unwrap();
        let peer3 = cfg.peers.iter().find(|p| p.name == "3").unwrap();

        assert_eq!(peer1.ips, vec![IpAddr::from_str("10::2").unwrap()]);
        assert_eq!(peer2.ips, vec![IpAddr::from_str("10::3").unwrap()]);
        assert_eq!(peer3.ips, vec![IpAddr::from_str("10::6").unwrap()]);
    }

    #[test]
    fn test_no_free_ip() {
        let mut cfg = new_config("10.0.0.0/32");
        add_peer(&mut cfg, "1").expect_err("Expected no free IPs");
    }

    #[test]
    fn test_one_free_ip() {
        let net = "10.0.0.0/24";
        let mut cfg = new_config(net);
        let free_ip = "10.0.0.25";
        for range in Ipv4Network::from_str(net)
            .unwrap()
            .subtract(&Ipv4Network::from_str(free_ip).unwrap())
        {
            ignore_range(&mut cfg, IpNetwork::V4(range)).unwrap();
        }

        add_peer(&mut cfg, "1").unwrap();
        let peer1 = cfg.peers.iter().find(|p| p.name == "1").unwrap();
        assert_eq!(
            peer1.ips,
            vec![IpAddr::V4(Ipv4Addr::from_str(free_ip).unwrap())]
        );
        add_peer(&mut cfg, "2").expect_err("Expected no free IPs");
    }

    #[test]
    fn test_ip_u32_limit_reached() {
        let net = "0.0.0.0/0";
        let mut cfg = new_config(net);
        ignore_range(&mut cfg, IpNetwork::from_str("0.0.0.0/1").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("128.0.0.0/1").unwrap()).unwrap();

        add_peer(&mut cfg, "1").expect_err("Expected no free IPs");
    }

    #[test]
    fn test_overlapping_ranges_1() {
        let net = "10.0.0.0/16";
        let mut cfg = new_config(net);
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/24").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/28").unwrap()).unwrap();
        assert_eq!(
            cfg.ignored_ipv4,
            HashSet::from_iter([Ipv4Network::from_str("10.0.0.0/24").unwrap()])
        );
    }

    #[test]
    fn test_overlapping_ranges_2() {
        let net = "10.0.0.0/16";
        let mut cfg = new_config(net);
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/24").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.1.0/24").unwrap()).unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/16").unwrap()).unwrap();
        assert_eq!(
            cfg.ignored_ipv4,
            HashSet::from_iter([Ipv4Network::from_str("10.0.0.0/16").unwrap()])
        );
    }

    #[test]
    fn test_ignore_assigned() {
        let net = "10.0.0.0/24";
        let mut cfg = new_config(net);
        add_peer(&mut cfg, "1").unwrap();
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/28").unwrap())
            .expect_err("Expected abort");
    }

    #[test]
    fn test_unignore_1() {
        let net = "10.0.0.0/24";
        let mut cfg = new_config(net);
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.0/24").unwrap()).unwrap();
        unignore_range(&mut cfg, IpNetwork::from_str("10.0.0.127/32").unwrap()).unwrap();
        add_peer(&mut cfg, "1").unwrap();
        let peer = cfg.peers.iter().find(|p| p.name == "1").unwrap();
        assert_eq!(peer.ips, vec![IpAddr::from_str("10.0.0.127").unwrap()]);
    }

    #[test]
    fn test_unignore_cancel() {
        let net = "10.0.0.0/24";
        let mut cfg = new_config(net);
        ignore_range(&mut cfg, IpNetwork::from_str("10.0.0.2/31").unwrap()).unwrap();
        unignore_range(&mut cfg, IpNetwork::from_str("10.0.0.2/31").unwrap()).unwrap();
        assert_eq!(cfg.ignored_ipv4, HashSet::new());
    }
}

// fn panic_hook(info: &std::panic::PanicInfo<'_>) {
//     println!("We panicked.");
//     println!("mowmow : {:?}", info.payload());
// }
