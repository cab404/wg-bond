// ! Wireguard conf file
use std::ops::{AddAssign};

use crate::configs::*;
use crate::wg_tools::*;

trait WGConfBuilder {
    fn cfg_param<P>(&mut self, name: &str, value: &P) where P: core::fmt::Display;
}

impl WGConfBuilder for String {
    fn cfg_param<P>(&mut self, name: &str, value: &P) where P: core::fmt::Display {
        self.add_assign(name);
        self.add_assign(" = ");
        self.add_assign(value.to_string().as_str());
        self.add_assign("\n");
    }
}

pub struct ConfFile {}

impl ConfigType for ConfFile {
    
    fn write_config(net: &WireguardNetworkInfo, id: u128) -> String { 
        
        let my_peer = net
          .peers
            .iter()
            .filter(|peer| peer.id == id)
            .next()
            .unwrap();
        let other_peers: Vec<&PeerInfo> = net
            .peers
            .iter()
            .filter(|peer| peer.id != id)
            .collect();
        let interface = net.map_to_interface(my_peer);
        

        let mut built = String::new();
        built.add_assign("[Interface]\n");
        for address in &interface.address {
            built.cfg_param("Address", &address);
        }
        
        match &interface.port {
            Some(port) => {
                built.cfg_param("ListenPort", port);                
            }
            _ => {}
        }

        built.cfg_param("PrivateKey", &interface.private_key);

        for peer in other_peers.iter() {
            match &peer.name {
                Some(a) => {
                    built.add_assign("[Peer] # ");
                    built.add_assign(a);
                    built.add_assign("\n");
                }
                None => {
                    built.add_assign("[Peer]\n");
                }
            }
            let b_peer = net.map_to_peer(peer);

            built.cfg_param("PublicKey", &gen_public_key(&peer.private_key));

            match &b_peer.endpoint {
                Some(ip) => {
                    built.cfg_param("Endpoint", &ip);
                }
                _ => {}
            }

            let ips = &b_peer.allowed_ips;
            
            if !ips.is_empty() {
                let nets: String = ips.iter()
                    .map(IpNetwork::to_string)
                    .collect::<Vec<String>>()
                    .join(&", ".to_string());
                built.cfg_param("AllowedIPs", &nets)
            }

        }
        built    
    }
    
    //  fn peer_config(interface: &Interface, peer: &Peer) -> String {  
    //     let mut built = String::new();
    //     built.add_assign("[Interface]\n");
    //     built.cfg_param("Address", &get_network_address(&interface.network, peer.id + 1));
    //     built.cfg_param("PrivateKey", &peer.private_key);
    //     built.add_assign("[Peer]\n");
    //     built.cfg_param("AllowedIPs", &interface.network);
    //     built.cfg_param("PublicKey", &gen_public_key(&interface.private_key));
    //     built
    // }

}