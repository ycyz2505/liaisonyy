use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

fn b64e(b: &[u8]) -> String {
    B64.encode(b)
}

fn b64d(s: &str) -> Vec<u8> {
    B64.decode(s.trim()).expect("invalid base64")
}

fn usage() {
    eprintln!(
        "usage:\n  update-sign keygen\n  update-sign sign <version> <min_client> <sha256_b64> <private_b64>\n  update-sign sha256 <file>"
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("keygen") => {
            let sk = SigningKey::generate(&mut rand::rngs::OsRng);
            println!("public={}", b64e(sk.verifying_key().as_bytes()));
            println!("private={}", b64e(sk.as_bytes()));
        }
        Some("sign") if args.len() == 6 => {
            let msg = format!(
                "liaisonyy-update-v1|{}|{}|{}",
                args[2], args[3], args[4]
            );
            let raw = b64d(&args[5]);
            assert_eq!(raw.len(), 32, "private key must be 32 bytes");
            let mut k = [0u8; 32];
            k.copy_from_slice(&raw);
            let sk = SigningKey::from_bytes(&k);
            let sig = sk.sign(msg.as_bytes());
            println!("{}", b64e(&sig.to_bytes()));
        }
        Some("sha256") if args.len() == 3 => {
            let data = std::fs::read(&args[2]).expect("read file");
            let mut h = Sha256::new();
            h.update(&data);
            println!("{}", b64e(&h.finalize()));
        }
        _ => usage(),
    }
}
