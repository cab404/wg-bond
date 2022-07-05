use base64;
use rand_core::OsRng;
use std::convert::TryFrom;
use x25519_dalek::{PublicKey, StaticSecret};

fn read_key(from: &Vec<u8>) -> String {
    String::from_utf8(from.to_owned())
        .unwrap()
        .trim_end()
        .to_string()
}

pub fn gen_private_key() -> String {
    base64::encode(StaticSecret::new(OsRng).to_bytes())
}

#[test]
pub fn test_on_regular_keys() {
    assert_eq!(
        gen_public_key("2JhyJzhRgEE9+lU7zPA8iLNvSwkJpHA2eTOndYR9BVs="),
        Ok("AM5SumUi+GKqTpHJM2lANpDwP0B0i1Ks+0aCCgnV0nU=".to_string())
    );
    assert_eq!(
        gen_public_key("dGVzdAo="),
        Err("Expected key size of 32, got 5".to_string())
    );
    assert_eq!(
        gen_public_key("JhyJzhRgEE9+lU7zPA8iLNvSwkJpHA2eTOndYR9BVs="),
        Err("Cannot decode base64".to_string())
    );
}

pub fn gen_public_key(private_key: &str) -> Result<String, String> {
    let private_base64 = base64::decode(private_key).map_err(|_| "Cannot decode base64")?;

    if private_base64.len() != 32 {
        return Err(format!(
            "Expected key size of 32, got {}",
            private_base64.len()
        ));
    }
    let mut private_sized: [u8; 32] = [0; 32];
    private_sized.clone_from_slice(&private_base64[..]);

    let secret = StaticSecret::try_from(private_sized).map_err(|_| "failed to convert keys?")?;

    Ok(base64::encode(PublicKey::from(&secret).as_bytes()))
}

pub fn gen_symmetric_key() -> String {
    // I hope private key is a valid symmetric key
    gen_private_key()
}
