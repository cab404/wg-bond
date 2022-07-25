use base64;
use rand::prelude::*;
use rand_core::{OsRng, RngCore};
use sha3::{Digest, Sha3_256};
use std::convert::TryFrom;
use x25519_dalek::{PublicKey, StaticSecret};

fn read_key(from: &Vec<u8>) -> String {
    String::from_utf8(from.to_owned())
        .unwrap()
        .trim_end()
        .to_string()
}

pub struct DerivationParameters {
    pub seed: String,
    pub path: String,
}

impl DerivationParameters {
    fn from(seed: &str, path: &str) -> DerivationParameters {
        DerivationParameters {
            seed: seed.to_string(),
            path: path.to_string(),
        }
    }
}

/// Generates private key, either by the seed or with [OsRng]
pub fn gen_private_key(seed: Option<DerivationParameters>) -> String {
    let mut key_bytes = [0u8; 32];
    match seed {
        Some(DerivationParameters { seed, path }) => {
            let mut hasher = Sha3_256::new();
            hasher.update(path);
            hasher.update(seed);
            let seed = hasher.finalize();

            let mut rng = StdRng::from_seed(seed.into());
            rng.fill(&mut key_bytes[..]);
        }
        None => OsRng.fill_bytes(&mut key_bytes),
    }
    base64::encode(StaticSecret::from(key_bytes).to_bytes())
}

#[test]
pub fn test_key_derivation() {
    assert_eq!(
        gen_private_key(Some(DerivationParameters::from(
            "Gandalf-melpa-celestia",
            "/0/1"
        ))),
        "cMnQ4piNjIClKG0DjnCwLsJlsR7W9Xr5qm9rwT/kE2I="
    );
    assert_eq!(
        gen_private_key(Some(DerivationParameters::from(
            "Gandalf-melpa-celestia",
            "/0/2"
        ))),
        "sGkJYyfOtCIs9I7Ue4EpQPiJytL6CZWGeNJcfIn1GV4="
    );
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
