use std::str::FromStr;

use url::Host;

pub(crate) const GLOBAL_NET_V4: &[&str; 30] = &[
    "0.0.0.0/5",
    "8.0.0.0/7",
    "11.0.0.0/8",
    "12.0.0.0/6",
    "16.0.0.0/4",
    "32.0.0.0/3",
    "64.0.0.0/2",
    "128.0.0.0/3",
    "160.0.0.0/5",
    "168.0.0.0/6",
    "172.0.0.0/12",
    "172.32.0.0/11",
    "172.64.0.0/10",
    "172.128.0.0/9",
    "173.0.0.0/8",
    "174.0.0.0/7",
    "176.0.0.0/4",
    "192.0.0.0/9",
    "192.128.0.0/11",
    "192.160.0.0/13",
    "192.169.0.0/16",
    "192.170.0.0/15",
    "192.172.0.0/14",
    "192.176.0.0/12",
    "192.192.0.0/10",
    "193.0.0.0/8",
    "194.0.0.0/7",
    "196.0.0.0/6",
    "200.0.0.0/5",
    "208.0.0.0/4",
];

// not yet sure :/
pub(crate) const GLOBAL_NET_V6: &[&str; 1] = &["::/0"];

/// Checks if endpoint is a valid ip or domain, and extracts port from it.
/// ```
/// assert_eq!(parse_url("test:8080"), Some((Host::Domain("test".to_string()), 8080)));
/// ```
pub(crate) fn split_endpoint(address: String) -> Result<(Host, u16), String> {
    let split = address
        .rsplitn(2, ':')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    match split.len() {
        1 => Err("You forgot the port.".to_string()),
        2 => Ok((
            Host::parse(split[0]).map_err(|f| f.to_string())?,
            u16::from_str(split[1]).map_err(|_| "Port number is weird.")?,
        )),
        _ => panic!(),
    }
}
#[test]
fn test_parse_endpoint() {
    assert_eq!(
        split_endpoint("test:8080".to_string()),
        Ok((Host::Domain("test".into()), 8080))
    );
    assert_eq!(
        split_endpoint("@:8080".to_string()),
        Err("invalid domain character".to_string())
    );
}
