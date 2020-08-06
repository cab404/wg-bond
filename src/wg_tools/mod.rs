use std::io::Write;
use std::process::{Command, Stdio};

fn read_key(from: &Vec<u8>) -> String {
    String::from_utf8(from.to_owned())
        .unwrap()
        .trim_end()
        .to_string()
}

pub fn gen_private_key() -> Result<String, String> {
    let key_bytes = Command::new("wg")
        .arg("genkey")
        .output()
        .map_err(|e| format!("Failed to run 'wg genkey': {}", e))?
        .stdout;

    Ok(read_key(&key_bytes))
}

pub fn gen_public_key(private_key: &str) -> Result<String, String> {
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run 'wg pubkey': {}", e))?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(private_key.as_bytes())
        .unwrap();

    let out = child.wait_with_output().unwrap();
    Ok(read_key(&out.stdout))
}
