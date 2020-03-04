use std::process::{Command, Stdio};
use std::io::Write;

fn read_key(from: &Vec<u8>) -> String {
    String::from_utf8(from.to_owned()).unwrap()
        .trim_end()
        .to_string()
}

pub fn gen_private_key() -> String {
    let key_bytes =
        Command::new("wg")
            .arg("genkey")
            .output()
            .expect("Failed to run 'wg genkey'.")
            .stdout;

    read_key(&key_bytes)
}

pub fn gen_public_key(private_key: &String) -> String {
    let mut child =
        Command::new("wg")
            .arg("pubkey")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

    child.stdin.as_mut().unwrap()
        .write(private_key.as_bytes())
        .unwrap();

    let out = child.wait_with_output().unwrap();
    read_key(&out.stdout)
}
