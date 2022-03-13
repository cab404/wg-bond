// ! Wireguard conf file
// Better way of doing this is invoking builtins.fromJSON, but that's not portable.

use crate::configs::*;

pub struct NixConf {}

#[derive(Debug, Clone)]

pub struct KeyFileExportConfig {
    /// What directory to use while exporting keyfiles
    pub target_prefix: String,
}

#[derive(Debug, Clone)]
pub struct NixExportConfig {
    pub use_keyfile: Option<KeyFileExportConfig>,
}

impl ConfigType for NixConf {
    type ExportConfig = NixExportConfig;

    fn write_config(config: WireguardConfiguration, export_options: NixExportConfig) -> String {
        let interface = config.interface;

        fn set_assign(key: &str, value: &Option<impl core::fmt::Display>) -> String {
            match value {
                Some(val) => format!("{}=\"{}\";", key, val),
                _ => "".into(),
            }
        }

        fn set_assign_raw(key: &str, value: &Option<impl core::fmt::Display>) -> String {
            match value {
                Some(val) => format!("{}={};", key, val),
                _ => "".into(),
            }
        }

        let mut built = String::new();
        built += format!("networking.wg-quick.interfaces.\"{}\"={{", &config.name).as_str();

        if let Some(KeyFileExportConfig { target_prefix }) = export_options.use_keyfile {
            built += format!(
                "privateKeyFile=\"{}/wg-{}.ed25519.base64\";",
                target_prefix, &config.name
            )
            .as_str();
        } else {
            built += format!("privateKey=\"{}\";", &interface.private_key).as_str();
        }

        built += set_assign_raw("listenPort", &interface.port).as_str();

        fn wrap_string<T>(thing: &T) -> String
        where
            T: core::fmt::Display,
        {
            format!("\"{}\"", thing)
        }

        // Addresses
        built += format!(
            "address=[{}];",
            &interface
                .address
                .iter()
                .map(wrap_string)
                .collect::<Vec<String>>()
                .join(" ")
        )
        .as_str();

        if !interface.dns.is_empty() {
            built += format!(
                "dns=[{}];",
                &interface
                    .dns
                    .iter()
                    .map(wrap_string)
                    .collect::<Vec<String>>()
                    .join(" ")
            )
            .as_str()
        }

        built += set_assign("preUp", &interface.pre_up).as_str();
        built += set_assign("preDown", &interface.pre_down).as_str();
        built += set_assign("postUp", &interface.post_up).as_str();
        built += set_assign("postDown", &interface.post_down).as_str();

        // Peers
        fn encode_peer(peer: &Peer) -> String {
            let mut built = String::new();
            built += "{";
            built += set_assign("publicKey", &Some(&peer.public_key)).as_str();
            built += format!(
                "allowedIPs=[{}];",
                peer.allowed_ips
                    .iter()
                    .map(wrap_string)
                    .collect::<Vec<String>>()
                    .join(" ")
            )
            .as_str();
            built += set_assign_raw("persistentKeepalive", &peer.persistent_keepalive).as_str();
            built += set_assign("presharedKey", &peer.preshared_key).as_str();
            built += set_assign("endpoint", &peer.endpoint).as_str();
            built += "}";
            built
        }

        built += format!(
            "peers=[{}];",
            config
                .peers
                .iter()
                .map(encode_peer)
                .collect::<Vec<String>>()
                .join(" ")
        )
        .as_str();

        built += "};";

        built
    }
}
