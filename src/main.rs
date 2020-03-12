extern crate base64;
extern crate rand;
extern crate serde;
extern crate serde_json;
extern crate qrcode;

use crate::configs::check_endpoint;
use crate::configs::{ConfigType, ConfigWriter};
use std::str::FromStr;
use ipnetwork::{IpNetwork};

use configs::nix::NixConf;
use configs::qr::QRConfig;
use configs::conf::ConfFile;

use clap;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

mod wg_tools;
mod configs;
use std::iter::*;

fn read_config(fname: &str) -> Result<configs::WireguardNetworkInfo, String> {
    debug!("Opening config from {}", fname);

    match std::fs::OpenOptions::new().create(false).read(true).open(fname) {
        Ok(handle) => {
            let result : Result<configs::WireguardNetworkInfo, serde_json::Error> = serde_json::from_reader(std::io::BufReader::new(handle));
            match result {
                Ok(net) => {
                    Ok(net)
                }
                Err(serde_err) => {
                    Err(format!("Cannot deserialize config file, {}", serde_err))
                }
            }
        }
        Err(fs_err) => {
            Err(format!("Cannot open config file, {}", fs_err))
        }
    }

}

fn save_config(cfg: &configs::WireguardNetworkInfo, fname: &str) -> Result<(), std::io::Error> {
    std::fs::OpenOptions::new()
        .create(true).write(true).truncate(true).open(fname)
        .map(|f| std::io::BufWriter::new(f) )
        .and_then(|writer| Ok(serde_json::to_writer_pretty(writer, cfg).unwrap()))
}

fn new_id(cfg: &configs::WireguardNetworkInfo) -> u128 {
    cfg.peers.iter().map(|i| i.id).max().unwrap_or(0) + 1
}

fn command_init_config(matches: &clap::ArgMatches) -> configs::WireguardNetworkInfo {

    let name: &str = matches.value_of("name").unwrap();
    let net: &str = matches.value_of("network").unwrap();

    configs::WireguardNetworkInfo {
        name: name.to_string(),
        network: IpNetwork::from_str(net).unwrap(),
        peers: vec![]
    }

}

fn command_new_peer(cfg: &mut configs::WireguardNetworkInfo, matches: &clap::ArgMatches) -> Result<(), u8>  {
    let peer_id = new_id(cfg);
    let name: String = matches.value_of("name").unwrap().into();
    let endpoint = matches.value_of("endpoint").and_then(|a| check_endpoint(&a.into()));

    cfg.peers.append(&mut vec![configs::PeerInfo {
        name: Some(name),
        endpoint: endpoint,
        id: peer_id,
        private_key: wg_tools::gen_private_key(),
        flags: vec![]
    }]);

    info!("Peer with id {id} added!", id = peer_id);

    Ok(())
}

fn command_export(cfg: &configs::WireguardNetworkInfo, matches: &clap::ArgMatches, exporter: ConfigWriter) -> Result<(), u8> {
    let id = u128::from_str(matches.value_of("id").unwrap()).unwrap();
    if let Some(_) = cfg.by_id(id) {
        println!("{}", exporter(&cfg, id));
        Ok(())
    } else {
        error!("Peer (id={id}) not found!", id=id);
        Err(1)
    }
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
            clap::SubCommand::with_name("init")
                .about("Initializes a config file")
                .arg(clap::Arg::with_name("name")
                    .help("Network name")
                    .required(true)
                )
                .arg(clap::Arg::with_name("network")
                    .short("n")
                    .long("network")
                    .help("Network for peers to use")
                    .value_name("IP/MASK")
                    .default_value("10.0.0.0/24")
                    .use_delimiter(false)
                    .takes_value(true)
                )
        )
        .subcommand(
            clap::SubCommand::with_name("add")
                .about("Adds a new peer to the network")
                .arg(clap::Arg::with_name("name")
                    .help("Name for a new peer")
                    .required(true)
                )
                .arg(clap::Arg::with_name("endpoint")
                    .short("e")
                    .long("endpoint")
                    .help("Endpoint address of a peer")
                    .value_name("example.com:8080")
                    .use_delimiter(false)
                    .takes_value(true)
                )

        )
        .subcommand(
            clap::SubCommand::with_name("nix")
                .about("Generates Nix configs")
                .arg(clap::Arg::with_name("id")
                    .help("Id of a peer")
                )
        )
        .subcommand(
            clap::SubCommand::with_name("qr")
                .about("Generates QR code with config")
                .arg(clap::Arg::with_name("id")
                    .help("Id of a peer")
                )
        )
        .subcommand(
            clap::SubCommand::with_name("conf")
                .about("Generates wg-quick configs")
                .arg(clap::Arg::with_name("id")
                    .help("Id of a peer")
                )
        )
        .get_matches();

    let cfg_file = args.value_of("config").unwrap();

    let mut net =
        if let Some(matches) = args.subcommand_matches("init") {
            command_init_config(matches)
        } else {
            read_config(cfg_file).unwrap()
        };

    fn commands(net: &mut configs::WireguardNetworkInfo, args: &clap::ArgMatches) -> Result<(), u8> {

        if let Some(matches) = args.subcommand_matches("add") {
            command_new_peer(net, matches)?;
        }

        if let Some(matches) = args.subcommand_matches("nix") {
            command_export(net, matches, NixConf::write_config)?;
        }

        if let Some(matches) = args.subcommand_matches("conf") {
            command_export(net, matches, ConfFile::write_config)?;
        }

        if let Some(matches) = args.subcommand_matches("qr") {
            command_export(net, matches, QRConfig::write_config)?;
        }

        Ok(())

    }

    commands(&mut net, &args).unwrap();

    save_config(&net, cfg_file).unwrap();

}

