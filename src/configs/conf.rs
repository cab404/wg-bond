// ! Wireguard conf file
use std::ops::AddAssign;

use crate::configs::*;

trait WGConfBuilder {
    fn cfg_param(&mut self, name: &str, value: impl core::fmt::Display);
    fn cfg_param_opt(&mut self, name: &str, value: Option<impl core::fmt::Display>);
    fn cfg_write_list(&mut self, name: &str, list: Vec<impl core::fmt::Display>);
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

    fn cfg_write_list(&mut self, name: &str, list: Vec<impl core::fmt::Display>) {
        if !list.is_empty() {
            self.cfg_param(
                name,
                list.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
}

pub struct ConfFile {}

impl ConfigType for ConfFile {
    fn write_config(config: WireguardConfiguration) -> String {
        let interface = config.interface;

        let mut built = String::new();
        built.add_assign("[Interface]\n");
        built.cfg_param("PrivateKey", &interface.private_key);

        built.cfg_write_list("Address", interface.address);
        built.cfg_write_list("DNS", interface.dns);
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
            built.cfg_write_list(
                "AllowedIPs",
                peer.allowed_ips
                    .iter()
                    .map(IpNetwork::to_string)
                    .collect::<Vec<_>>(),
            );
        }
        built
    }
}
