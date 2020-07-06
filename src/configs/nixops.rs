use crate::configs::nix::NixConf;
use crate::configs::*;

pub struct NixOpsConf {}

impl ConfigType for NixOpsConf {
    fn write_config(net: &WireguardNetworkInfo, _id: u128) -> String {
        // TODO: Don't just ignore id, and make write_config accept ArgMatches instead
        let mut built = String::new();

        built += "{";

        built += "defaults={networking.extraHosts=\"";
        built += hosts::export_hosts(net)
            .replace("\"", "\\\"")
            .replace("\n", "\\n")
            .replace("\t", "\\t")
            .as_str();
        built += "\";};";

        for peer in net.peers.iter().filter(|a| a.has_flag("NixOpsMachine")) {
            built += "\"";
            built += peer.name.as_str();
            built += "\".";
            built += NixConf::write_config(net, peer.id).as_str();
        }

        built += "}\n";

        built
    }
}
