// ! Wireguard conf file
use std::ops::AddAssign;

use crate::configs::*;

trait WGConfBuilder {
    fn cfg_param(&mut self, name: &str, value: impl core::fmt::Display);
    fn cfg_param_opt(&mut self, name: &str, value: Option<impl core::fmt::Display>);
}

impl WGConfBuilder for String {
    fn cfg_param(&mut self, name: &str, value: impl core::fmt::Display) {
        self.add_assign(name);
        self.add_assign(" = ");
        self.add_assign(value.to_string().as_str());
        self.add_assign("\n");
    }

    fn cfg_param_opt(&mut self, name: &str, optval: Option<impl core::fmt::Display>) {
        if let Some(value) = optval {
            self.add_assign(name);
            self.add_assign(" = ");
            self.add_assign(value.to_string().as_str());
            self.add_assign("\n");
        }
    }
}

pub struct ConfFile {}

impl ConfigType for ConfFile {
    fn write_config(net: &WireguardNetworkInfo, id: u128) -> String {
        let my_peer = net.by_id(id).unwrap();
        let config = net.get_configuration(my_peer);
        let interface = net.map_to_interface(my_peer);

        let mut built = String::new();
        built.add_assign("[Interface]\n");
        built.cfg_param("PrivateKey", &interface.private_key);
        for address in &interface.address {
            built.cfg_param("Address", &address);
        }
        built.cfg_param_opt("ListenPort", interface.port);
        built.cfg_param_opt("Table", interface.table);
        built.cfg_param_opt("PreUp", interface.pre_up);
        built.cfg_param_opt("PreDown", interface.pre_down);
        built.cfg_param_opt("PostUp", interface.post_up);
        built.cfg_param_opt("PostDown", interface.post_down);

        for peer in config.peers.iter() {
            built.add_assign("[Peer]");
            built.add_assign("\n");

            built.cfg_param("PublicKey", &peer.public_key);
            built.cfg_param_opt("PresharedKey", peer.preshared_key.as_ref());
            built.cfg_param_opt("Endpoint", peer.endpoint.as_ref());
            built.cfg_param_opt("PersistentKeepalive", peer.persistent_keepalive);

            let ips = &peer.allowed_ips;
            if !ips.is_empty() {
                let nets: String = ips
                    .iter()
                    .map(IpNetwork::to_string)
                    .collect::<Vec<String>>()
                    .join(&", ".to_string());
                built.cfg_param("AllowedIPs", &nets)
            }
        }
        built
    }
}
