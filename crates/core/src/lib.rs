use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: i64 = 1;
pub const APP_VERSION: &str = "0.3.1";

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("bad base64")]
    Base64,
    #[error("crypto failure")]
    Crypto,
    #[error("bad kdf params")]
    Kdf,
    #[error("bad sealed box")]
    Sealed,
    #[error("bad json")]
    Json(#[from] serde_json::Error),
}
pub type CoreResult<T> = std::result::Result<T, CoreError>;

impl From<CoreError> for String {
    fn from(e: CoreError) -> Self {
        e.to_string()
    }
}

/// Network-anchored clock. The server initialises it from NTP/HTTP; clients
/// initialise it from the server's `server_time`. All timestamps then use the
/// monotonic elapsed time since the anchor, so changing the local wall clock
/// does not affect message/expiry times.
pub mod clock {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    struct Anchor {
        unix: i64,
        at: Instant,
    }

    static ANCHOR: OnceLock<Mutex<Option<Anchor>>> = OnceLock::new();

    pub fn init_network_time(unix: i64) {
        let lock = ANCHOR.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = lock.lock() {
            *guard = Some(Anchor { unix, at: Instant::now() });
        }
    }

    pub fn is_synced() -> bool {
        ANCHOR
            .get()
            .and_then(|lock| lock.lock().ok().map(|g| g.is_some()))
            .unwrap_or(false)
    }

    pub fn now() -> i64 {
        if let Some(lock) = ANCHOR.get() {
            if let Ok(guard) = lock.lock() {
                if let Some(anchor) = guard.as_ref() {
                    return anchor.unix + anchor.at.elapsed().as_secs() as i64;
                }
            }
        }
        time::OffsetDateTime::now_utc().unix_timestamp()
    }
}

pub fn b64e(b: &[u8]) -> String {
    B64.encode(b)
}
pub fn b64d(s: &str) -> CoreResult<Vec<u8>> {
    B64.decode(s).map_err(|_| CoreError::Base64)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KdfParams {
    pub m: u32,
    pub t: u32,
    pub p: u32,
}
impl Default for KdfParams {
    fn default() -> Self {
        Self { m: 32768, t: 3, p: 1 }
    }
}

/// Argon2id -> (KEK, verify). KEK stays in memory; verify is stored.
pub fn derive_keys(secret: &str, salt_b64: &str, params: &KdfParams) -> CoreResult<([u8; 32], [u8; 32])> {
    let salt = b64d(salt_b64)?;
    let a = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(params.m, params.t, params.p, Some(64)).map_err(|_| CoreError::Kdf)?,
    );
    let mut out = [0u8; 64];
    a.hash_password_into(secret.as_bytes(), &salt, &mut out)
        .map_err(|_| CoreError::Kdf)?;
    let mut kek = [0u8; 32];
    kek.copy_from_slice(&out[..32]);
    let mut verify = [0u8; 32];
    verify.copy_from_slice(&out[32..]);
    out.fill(0);
    Ok((kek, verify))
}

pub fn new_salt() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b64e(&b)
}

pub fn gen_keypair() -> (String, String) {
    use rand::rngs::OsRng;
    let sk = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let pk = x25519_dalek::PublicKey::from(&sk);
    (b64e(sk.as_bytes()), b64e(pk.as_bytes()))
}

pub fn pk_from_sk(sk_b64: &str) -> CoreResult<String> {
    let sk = b64d(sk_b64)?;
    if sk.len() != 32 {
        return Err(CoreError::Crypto);
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&sk);
    let sk = x25519_dalek::StaticSecret::from(arr);
    Ok(b64e(x25519_dalek::PublicKey::from(&sk).as_bytes()))
}

fn aead_key(key: &[u8; 32]) -> aes_gcm::Aes256Gcm {
    use aes_gcm::KeyInit;
    aes_gcm::Aes256Gcm::new_from_slice(key).expect("32-byte key")
}

pub fn aead_encrypt(key: &[u8; 32], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    use aes_gcm::aead::{Aead, Payload};
    use rand::RngCore;
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ct = aead_key(key)
        .encrypt(
            aes_gcm::Nonce::from_slice(&nonce),
            Payload { msg: plaintext, aad },
        )
        .expect("aead encrypt");
    let mut out = Vec::with_capacity(12 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

pub fn aead_decrypt(key: &[u8; 32], data: &[u8], aad: &[u8]) -> CoreResult<Vec<u8>> {
    use aes_gcm::aead::{Aead, Payload};
    if data.len() < 13 {
        return Err(CoreError::Crypto);
    }
    aead_key(key)
        .decrypt(
            aes_gcm::Nonce::from_slice(&data[..12]),
            Payload { msg: &data[12..], aad },
        )
        .map_err(|_| CoreError::Crypto)
}

pub fn wrap_key(kek: &[u8; 32], key: &[u8]) -> String {
    b64e(&aead_encrypt(kek, key, b"liaisonyy-wrap-v1"))
}
pub fn unwrap_key(kek: &[u8; 32], wrapped: &str) -> CoreResult<Vec<u8>> {
    aead_decrypt(kek, &b64d(wrapped)?, b"liaisonyy-wrap-v1")
}

pub fn seal(recipient_pk_b64: &str, plaintext: &[u8]) -> CoreResult<String> {
    use rand::rngs::OsRng;
    let pk_bytes = b64d(recipient_pk_b64)?;
    if pk_bytes.len() != 32 {
        return Err(CoreError::Sealed);
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&pk_bytes);
    let recipient = x25519_dalek::PublicKey::from(arr);
    let esk = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let epk = x25519_dalek::PublicKey::from(&esk);
    let shared = esk.diffie_hellman(&recipient);
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"liaisonyy-seal-v1"), shared.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"", &mut key).map_err(|_| CoreError::Sealed)?;
    let body = aead_encrypt(&key, plaintext, b"liaisonyy-seal-v1");
    key.fill(0);
    let mut out = Vec::with_capacity(32 + body.len());
    out.extend_from_slice(epk.as_bytes());
    out.extend_from_slice(&body);
    Ok(b64e(&out))
}

pub fn open_sealed(sk_b64: &str, sealed_b64: &str) -> CoreResult<Vec<u8>> {
    let sk_bytes = b64d(sk_b64)?;
    let data = b64d(sealed_b64)?;
    if sk_bytes.len() != 32 || data.len() < 45 {
        return Err(CoreError::Sealed);
    }
    let mut sk_arr = [0u8; 32];
    sk_arr.copy_from_slice(&sk_bytes);
    let sk = x25519_dalek::StaticSecret::from(sk_arr);
    let mut epk_arr = [0u8; 32];
    epk_arr.copy_from_slice(&data[..32]);
    let epk = x25519_dalek::PublicKey::from(epk_arr);
    let shared = sk.diffie_hellman(&epk);
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"liaisonyy-seal-v1"), shared.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"", &mut key).map_err(|_| CoreError::Sealed)?;
    let out = aead_decrypt(&key, &data[32..], b"liaisonyy-seal-v1");
    key.fill(0);
    out
}

pub fn random_code(len: usize) -> String {
    use rand::Rng;
    let mut r = rand::thread_rng();
    (0..len)
        .map(|_| char::from(b'0' + r.gen_range(0..10)))
        .collect()
}

pub fn now() -> i64 {
    clock::now()
}

pub fn sha256_b64(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    b64e(&h.finalize())
}

pub fn hmac_sha256_b64(key: &[u8], msg: &[u8]) -> String {
    use hmac::Mac;
    let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(key).expect("hmac key");
    mac.update(msg);
    b64e(&mac.finalize().into_bytes())
}

pub fn random_token() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b64e(&b)
}

pub fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------- protocol ----------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClassInfo {
    pub id: i64,
    pub name: String,
    pub grade: String,
    pub escrow_public_key: Option<String>,
    pub unclaimed: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BootstrapResponse {
    pub setup_required: bool,
    pub setup_token_required: bool,
    pub school_name: String,
    pub server_name: String,
    pub version: String,
    pub protocol: i64,
    pub server_time: i64,
    pub root_public_key: Option<String>,
    pub kdf_params: KdfParams,
    pub classes: Vec<ClassInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RosterEntry {
    pub id: i64,
    pub real_name: String,
    pub student_no: Option<String>,
    pub claimed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AccountPublic {
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub class_id: i64,
    pub role: String,
    pub login_policy: String,
    pub public_key: String,
    pub avatar_file_id: Option<i64>,
    pub unread: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PublicProfile {
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub real_name: String,
    pub class_id: Option<i64>,
    pub class_name: Option<String>,
    pub role: String,
    pub avatar_file_id: Option<i64>,
    pub background_file_id: Option<i64>,
    pub signature: String,
    pub bio: String,
    pub gender: Option<String>,
    pub birthday: Option<String>,
    pub socials_json: Option<String>,
    pub is_friend: bool,
    pub is_blocked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetupRequest {
    pub setup_token: String,
    pub handle: String,
    pub display_name: String,
    pub password: String,
    pub public_key: String,
    pub pwd_salt: String,
    pub pwd_params: KdfParams,
    pub root_own: String,
    pub root_recovery: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegisterRequest {
    pub class_id: i64,
    pub roster_id: i64,
    pub real_name: String,
    pub handle: String,
    pub display_name: String,
    pub password: String,
    pub login_policy: String,
    pub pin: Option<String>,
    pub public_key: String,
    pub pwd_salt: String,
    pub pwd_params: KdfParams,
    pub pin_salt: Option<String>,
    pub pin_params: Option<KdfParams>,
    pub own_priv: String,
    pub pin_own: Option<String>,
    pub user_priv_adm: String,
    pub user_priv_class: String,
    pub profile_own: String,
    pub profile_pin: Option<String>,
    pub profile_adm: String,
    pub profile_class: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LoginRequest {
    pub uid_or_handle: String,
    pub kind: String, // password | pin
    pub input: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LoginResponse {
    pub token: String,
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub real_name: String,
    pub class_id: Option<i64>,
    pub role: String,
    pub login_policy: String,
    pub public_key: String,
    pub avatar_file_id: Option<i64>,
    pub server_time: i64,
    pub matched_offset: usize,
    pub matched_len: usize,
    pub pwd_salt: String,
    pub pwd_params: KdfParams,
    pub pin_salt: Option<String>,
    pub pin_params: Option<KdfParams>,
    pub own_priv: String,
    pub pin_own: Option<String>,
    pub user_priv_adm: String,
    pub user_priv_class: String,
    pub profile_own: String,
    pub profile_pin: Option<String>,
    pub profile_adm: String,
    pub profile_class: String,
    pub profile_enc: String,
    pub classes: Vec<ClassInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MeResponse {
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub real_name: String,
    pub class_id: Option<i64>,
    pub role: String,
    pub login_policy: String,
    pub public_key: String,
    pub avatar_file_id: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConversationInfo {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub class_id: Option<i64>,
    pub key_version: i64,
    pub member_wrap: String,
    pub unread: i64,
    pub last_at: i64,
    pub last_preview: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MessageDto {
    pub id: i64,
    pub conv_id: i64,
    pub sender_uid: i64,
    pub sender_name: String,
    pub key_version: i64,
    pub client_msg_id: String,
    pub ciphertext: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SendMessageRequest {
    pub key_version: i64,
    pub ciphertext: String,
    pub client_msg_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FriendRequestDto {
    pub id: i64,
    pub from_uid: i64,
    pub from_name: String,
    pub from_public_key: String,
    pub to_uid: i64,
    pub to_name: String,
    pub message_enc: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FriendDto {
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub public_key: String,
    pub avatar_file_id: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdminStats {
    pub users: i64,
    pub online: i64,
    pub messages_today: i64,
    pub messages_total: i64,
    pub classes: i64,
    pub pending_requests: i64,
    pub db_bytes: i64,
    pub reports_pending: i64,
    pub muted: i64,
    pub banned: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NotificationDto {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub created_at: i64,
    pub read: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnnouncementDto {
    pub id: i64,
    pub scope: String,
    pub class_id: Option<i64>,
    pub class_name: Option<String>,
    pub title: String,
    pub body: String,
    pub created_by: i64,
    pub created_by_name: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateAnnouncementRequest {
    pub scope: String,
    pub class_id: Option<i64>,
    pub title: String,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReportDto {
    pub id: i64,
    pub reporter_uid: i64,
    pub reporter_name: String,
    pub target_type: String,
    pub target_id: i64,
    pub target_uid: Option<i64>,
    pub target_name: Option<String>,
    pub message_id: Option<i64>,
    pub conv_id: Option<i64>,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
    pub handled_by: Option<i64>,
    pub action: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateReportRequest {
    pub target_type: String,
    pub target_id: i64,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResolveReportRequest {
    pub action: String,
    pub minutes: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserActionRequest {
    pub action: String,
    pub minutes: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransferDto {
    pub id: i64,
    pub uid: i64,
    pub display_name: String,
    pub from_class_id: i64,
    pub from_class_name: String,
    pub to_class_id: i64,
    pub to_class_name: String,
    pub reason: String,
    pub status: String,
    pub out_by: Option<i64>,
    pub in_by: Option<i64>,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateTransferRequest {
    pub to_class_id: i64,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransferActionRequest {
    pub action: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditVerify {
    pub ok: bool,
    pub checked: i64,
    pub first_bad: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClassStat {
    pub class_id: i64,
    pub name: String,
    pub users: i64,
    pub messages_today: i64,
    pub messages_total: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdminDashboard {
    pub users: i64,
    pub online: i64,
    pub sessions: i64,
    pub auth_fail_today: i64,
    pub messages_today: i64,
    pub messages_total: i64,
    pub classes: Vec<ClassStat>,
    pub db_bytes: i64,
    pub media_bytes: i64,
    pub reports_pending: i64,
    pub muted: i64,
    pub banned: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupInfo {
    pub name: String,
    pub created_at: i64,
    pub bytes: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupResult {
    pub name: String,
    pub path: String,
    pub bytes: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdminMessageDto {
    pub id: i64,
    pub sender_uid: i64,
    pub sender_name: String,
    pub text: String,
    pub key_version: i64,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdminDecrypted {
    pub conv_id: i64,
    pub messages: Vec<AdminMessageDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiscoveredServer {
    pub url: String,
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdatePackageRequest {
    pub data: String,
    pub version: String,
    pub min_client: String,
    pub signature: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditEntry {
    pub id: i64,
    pub ts: i64,
    pub actor_uid: Option<i64>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateClassRequest {
    pub name: String,
    pub grade: String,
    pub escrow_public_key: String,
    pub escrow_priv_for_root: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImportRosterRequest {
    pub names: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AssignClassAdminRequest {
    pub uid: i64,
    pub class_priv_for_admin: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateResetKeyRequest {
    pub uid: i64,
    pub code: String,
    pub expires_at: i64,
    pub reset_salt: String,
    pub code_wrap_root: String,
    pub reset_wrap_priv: String,
    pub reset_wrap_profile: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResetFetchRequest {
    pub uid_or_handle: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResetFetchResponse {
    pub reset_salt: String,
    pub reset_wrap_priv: String,
    pub reset_wrap_profile: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyWrap {
    pub recipient_uid: i64,
    pub wrapped_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AcceptFriendRequest {
    pub key_version: i64,
    pub keys: Vec<KeyWrap>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FriendRequestCreate {
    pub to_uid: i64,
    pub message_enc: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub profile_enc: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReadRequest {
    pub last_read: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateProfileRequest {
    pub display_name: String,
    pub signature: String,
    pub bio: String,
    pub public_gender: String,
    pub public_birthday: String,
    pub public_socials: String,
    pub profile_enc: String,
    #[serde(default)]
    pub profile_own: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetProfileKeyRequest {
    pub profile_own: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RekeyClassRequest {
    pub user_priv_class: String,
    pub profile_class: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChangePasswordRequest {
    pub old_input: String,
    pub new_password: String,
    pub pwd_salt: String,
    pub pwd_params: KdfParams,
    pub own_priv: String,
    pub profile_own: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetPinRequest {
    pub password_input: String,
    pub pin: String,
    pub pin_salt: String,
    pub pin_params: KdfParams,
    pub pin_own: String,
    pub profile_pin: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetLoginPolicyRequest {
    pub password_input: String,
    pub policy: String,
    pub pin: Option<String>,
    pub pin_salt: Option<String>,
    pub pin_params: Option<KdfParams>,
    pub pin_own: Option<String>,
    pub profile_pin: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockRequest {
    pub uid: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AvatarRequest {
    pub data: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MyProfileResponse {
    pub uid: i64,
    pub handle: String,
    pub display_name: String,
    pub real_name: String,
    pub class_id: Option<i64>,
    pub role: String,
    pub login_policy: String,
    pub avatar_file_id: Option<i64>,
    pub background_file_id: Option<i64>,
    pub signature: String,
    pub bio: String,
    pub gender: Option<String>,
    pub birthday: Option<String>,
    pub socials_json: Option<String>,
    pub profile_enc: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResetRequest {
    pub uid_or_handle: String,
    pub code: String,
    pub new_password: String,
    pub new_pin: Option<String>,
    pub pwd_salt: String,
    pub pwd_params: KdfParams,
    pub own_priv: String,
    pub pin_own: Option<String>,
    pub pin_salt: Option<String>,
    pub pin_params: Option<KdfParams>,
    pub profile_own: String,
    pub profile_pin: Option<String>,
}
