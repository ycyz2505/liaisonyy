use liaisonyy_core::*;

#[test]
fn argon2_is_deterministic_and_verifies() {
    let salt = new_salt();
    let params = KdfParams::default();
    let (k1, v1) = derive_keys("Pass1234a", &salt, &params).unwrap();
    let (k2, v2) = derive_keys("Pass1234a", &salt, &params).unwrap();
    assert_eq!(k1, k2);
    assert_eq!(v1, v2);
    let (k3, v3) = derive_keys("Pass1234b", &salt, &params).unwrap();
    assert_ne!(k1, k3);
    assert_ne!(v1, v3);
}

#[test]
fn aead_detects_tampering() {
    let key = [7u8; 32];
    let ct = aead_encrypt(&key, b"secret", b"aad");
    assert_eq!(aead_decrypt(&key, &ct, b"aad").unwrap(), b"secret");
    let mut bad = ct.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(aead_decrypt(&key, &bad, b"aad").is_err());
    assert!(aead_decrypt(&key, &ct, b"other").is_err());
    assert!(aead_decrypt(&[8u8; 32], &ct, b"aad").is_err());
}

#[test]
fn wrapped_keys_require_the_right_kek() {
    let kek = [1u8; 32];
    let other = [2u8; 32];
    let wrapped = wrap_key(&kek, b"private-key-material");
    assert_eq!(unwrap_key(&kek, &wrapped).unwrap(), b"private-key-material");
    assert!(unwrap_key(&other, &wrapped).is_err());
}

#[test]
fn sealed_boxes_require_the_right_private_key() {
    let (sk_a, pk_a) = gen_keypair();
    let (sk_b, _pk_b) = gen_keypair();
    let sealed = seal(&pk_a, b"hello").unwrap();
    assert_eq!(open_sealed(&sk_a, &sealed).unwrap(), b"hello");
    assert!(open_sealed(&sk_b, &sealed).is_err());
}

#[test]
fn b64_roundtrip_and_bad_input() {
    let raw = [0u8, 1, 2, 250, 255];
    assert_eq!(b64d(&b64e(&raw)).unwrap(), raw);
    assert!(b64d("not base64 !!!").is_err());
}
