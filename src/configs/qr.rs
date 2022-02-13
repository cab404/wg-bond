// ! Wireguard conf file
// Better way of doing this is invoking builtins.fromJSON, but that's not portable.

use crate::configs::conf;
use crate::configs::*;

use qrcode::render::unicode;
use qrcode::QrCode;

pub struct QRConfig {}

impl ConfigType for QRConfig {
    type ExportConfig = ();

    fn write_config(net: WireguardConfiguration, export_options: ()) -> String {
        let cfg = conf::ConfFile::write_config(net, ());
        QrCode::new(&cfg)
            .unwrap()
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build()
    }
}
