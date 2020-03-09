// ! Wireguard conf file
// Better way of doing this is invoking builtins.fromJSON, but that's not portable.


use crate::configs::*;
use crate::configs::conf;

use qrcode::QrCode;
use qrcode::render::unicode;

pub struct QRConfig {}

impl ConfigType for QRConfig {

  fn write_config(net: &WireguardNetworkInfo, id: u128) -> String {
    let cfg = conf::ConfFile::write_config(&net, id);
    QrCode::new(&cfg).unwrap()
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build()
  }

}
