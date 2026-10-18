use liaisonyy_core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    #[serde(default = "default_server")]
    pub server: String,
    #[serde(default)]
    pub class_id: Option<i64>,
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub last_uid: Option<i64>,
    /// SHA-256 (base64) of the server certificate, keyed by host:port. TOFU.
    #[serde(default)]
    pub tls_pins: HashMap<String, String>,
}
fn default_server() -> String {
    "https://127.0.0.1:47820".to_string()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            server: default_server(),
            class_id: None,
            class_name: String::new(),
            last_uid: None,
            tls_pins: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PendingMessage {
    pub conv_id: i64,
    pub key_version: i64,
    pub ciphertext: String,
    pub client_msg_id: String,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Cache {
    #[serde(default)]
    pub login: Option<LoginResponse>,
    #[serde(default)]
    pub conversations: Vec<ConversationInfo>,
    #[serde(default)]
    pub messages: HashMap<String, Vec<MessageDto>>,
    #[serde(default)]
    pub outbox: Vec<PendingMessage>,
    #[serde(default)]
    pub last_ids: HashMap<String, i64>,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|x| x.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("data");
    if std::fs::create_dir_all(&dir).is_ok() {
        return dir;
    }
    let fallback = std::env::var("LOCALAPPDATA")
        .map(|s| PathBuf::from(s).join("liaisonyy"))
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

pub fn load_settings() -> Settings {
    let p = data_dir().join("settings.json");
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    let p = data_dir().join("settings.json");
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(p, t);
    }
}

const CACHE_MAGIC: &[u8; 5] = b"LYYC1";

#[cfg(windows)]
fn dpapi_protect(data: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &in_blob,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 {
        return None;
    }
    let out = unsafe {
        std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(out_blob.pbData as *mut _);
    }
    Some(out)
}

#[cfg(windows)]
fn dpapi_unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut out_blob,
        )
    };
    if ok == 0 {
        return None;
    }
    let out = unsafe {
        std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(out_blob.pbData as *mut _);
    }
    Some(out)
}

fn encrypt_cache(plain: &[u8]) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        let enc = dpapi_protect(plain)?;
        let mut out = CACHE_MAGIC.to_vec();
        out.extend_from_slice(&enc);
        Some(out)
    }
    #[cfg(not(windows))]
    {
        let _ = plain;
        None
    }
}

pub fn load_cache() -> Cache {
    let p = data_dir().join("cache.json");
    let Ok(bytes) = std::fs::read(&p) else {
        return Cache::default();
    };
    let plain: Vec<u8> = if bytes.starts_with(CACHE_MAGIC) {
        #[cfg(windows)]
        {
            match dpapi_unprotect(&bytes[CACHE_MAGIC.len()..]) {
                Some(v) => v,
                None => return Cache::default(),
            }
        }
        #[cfg(not(windows))]
        {
            return Cache::default();
        }
    } else {
        bytes
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn save_cache(c: &Cache) {
    let p = data_dir().join("cache.json");
    let Ok(t) = serde_json::to_vec(c) else {
        return;
    };
    let data = match encrypt_cache(&t) {
        Some(v) => v,
        None => t,
    };
    let tmp = p.with_extension("json.tmp");
    if std::fs::write(&tmp, &data).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}
