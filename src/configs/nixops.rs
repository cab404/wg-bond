use crate::configs::*;
use crate::configs::nix::{NixConf};

pub struct NixOpsConf {}

impl ConfigType for NixOpsConf {

  fn write_config(net: &WireguardNetworkInfo, id: u128) -> String {

    // TODO: Don't just ignore id, and make write_config accept ArgMatches instead
    let mut built = String::new();

    built += "{";

    for peer in net.peers.iter() {
      built += "\"";
      built += peer.name.as_str();
      built += "\".";
      built += NixConf::write_config(net, peer.id).as_str();
    }

    built += "}\n";

    built

  }
}
