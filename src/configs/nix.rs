// ! Wireguard conf file
// Better way of doing this is invoking builtins.fromJSON, but that's not portable.

use std::ops::AddAssign;

use crate::configs::*;
use crate::wg_tools::*;
use std::iter::*;
use std::borrow::Cow;

pub struct NixConf {}

impl ConfigType for NixConf {
  // fn write_config(info: &PeerInfo, peer: &Peer) -> String {

  //   }

  fn write_config(net: &WireguardNetworkInfo, id: u128) -> String {
    let my_peer = net
      .peers
      .iter()
      .filter(|peer| peer.id == id)
      .next()
      .unwrap();
    let other_peers: Vec<Peer> = net
      .peers
      .iter()
      .filter(|peer| peer.id != id)
      .map(|peer| net.map_to_peer(peer))
      .collect();

    fn write(key: &String, value: &Option<&String>) -> String {
      match value {
        Some(val) => {
          format!("{} = \"{}\";", key, val)
        }
        _ => {
          "".into()
        }
      }
    }

    let interface = net.map_to_interface(my_peer);

    let mut built = String::new();
    built += format!("networking.wg-quick.interfaces.\"{}\" = {{", &net.name).as_str();
    built += format!(" privateKey = \"{}\";", &my_peer.private_key).as_str();
    match &my_peer.port {
      Some(port) => {
        built += format!(" listenPort = \"{}\";", port).as_str();
      }
      _ => {}
    }
    
    // Addresses
    built += format!(" ips = [ {} ];\n", &my_peer.addresses.iter().map(|addr| format!("\"{}\"", addr.to_string())).collect::<Vec<String>>().join(" ")).as_str();

    // Peers
    fn encode_peer(peer: &Peer) -> String {
      let mut built = String::new();
      built += "{";
      built += format!(" publicKey = \"{}\";", peer.public_key).as_str();
      built += format!(" allowedIPs = [ {} ];", peer.allowed_ips.iter().map(|addr| format!("\"{}\"", addr.to_string())).collect::<Vec<String>>().join(" ")).as_str();     
      built += format!(" endpoints = [ {} ];", peer.endpoint.iter().map(|addr| format!("\"{}\"", addr.to_string())).collect::<Vec<String>>().join(" ")).as_str();     
      match peer.persistent_keepalive {
        Some(keepalive) => {
          built += format!(" persistentKeepalive = {};", keepalive).as_str();
        }
        _ => {}
      }
      built += "}";
      built
    }

    built += format!(" peers = [ {} ];", &other_peers.iter().map(|peer| encode_peer(peer)).collect::<Vec<String>>().join(", ")).as_str();

    /*
      networking.wg-quick.interfaces.wg0 = {
        privateKey = "...";
        ips = [ ... ];
        peers = [
          {
            allowedIPs = [ ... ];
            persistentKeepalive = 30;
            endpoint = "...";
            publicKey = "...";
          }
        ];
      };
    */
    // let mut built = String::new();
    // built += format!("networking.wg-quick.interfaces.{} = {{", &server.name).as_str();
    // built += format!("privateKey = \"{}\";", &server.private_key).as_str();
    // built += "ips = [";
    // built += "];";
    built += "};";


    built

  }
}
