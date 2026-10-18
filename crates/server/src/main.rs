#![cfg_attr(windows, windows_subsystem = "windows")]

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use liaisonyy_core::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket},
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const ADMIN_HTML: &str = include_str!("../../../admin-web/index.html");

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    data: PathBuf,
    hmac: Arc<Vec<u8>>,
    media_key: Arc<[u8; 32]>,
    ws: tokio::sync::broadcast::Sender<()>,
    rate: Arc<Mutex<HashMap<(i64, String), (i64, i64)>>>,
    setup_token: Arc<Mutex<String>>,
    version: Arc<String>,
}

#[derive(Clone)]
struct AuthUser {
    uid: i64,
    role: String,
    class_id: Option<i64>,
}

struct AppError {
    status: StatusCode,
    code: String,
    message: String,
}

impl AppError {
    fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self { status, code: code.into(), message: message.into() }
    }
    fn bad(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", m)
    }
    fn unauthorized(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", m)
    }
    fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", m)
    }
    fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", m)
    }
    fn conflict(m: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", m)
    }
    fn internal(m: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", m)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody { code: self.code, message: self.message }),
        )
            .into_response()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(format!("db: {e}"))
    }
}
impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        Self::internal(format!("crypto: {e}"))
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::internal(format!("json: {e}"))
    }
}
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(e.to_string())
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}

type ApiResult<T> = std::result::Result<T, AppError>;

fn validate_handle(h: &str) -> ApiResult<()> {
    let n = h.chars().count();
    if !(3..=20).contains(&n) {
        return Err(AppError::bad("YY 号需 3-20 位"));
    }
    if !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::bad("YY 号只能包含字母、数字、下划线"));
    }
    if h.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::bad("YY 号不能是纯数字"));
    }
    Ok(())
}

fn validate_password(p: &str) -> ApiResult<()> {
    let n = p.chars().count();
    if !(8..=64).contains(&n) {
        return Err(AppError::bad("密码需 8-64 位"));
    }
    if !p.chars().any(|c| c.is_ascii_alphabetic()) || !p.chars().any(|c| c.is_ascii_digit()) {
        return Err(AppError::bad("密码需同时包含字母和数字"));
    }
    Ok(())
}

fn validate_pin(p: &str) -> ApiResult<()> {
    let n = p.chars().count();
    if !(4..=20).contains(&n) || !p.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::bad("PIN 需 4-20 位纯数字"));
    }
    Ok(())
}

fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
        .optional()
}

fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn audit(
    conn: &Connection,
    key: &[u8],
    actor: Option<i64>,
    action: &str,
    tt: Option<&str>,
    tid: Option<&str>,
    detail: Option<&str>,
) -> rusqlite::Result<()> {
    let prev: String = conn
        .query_row("SELECT hash FROM audit_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    let ts = now();
    let payload = format!(
        "{}|{}|{}|{}|{}|{}|{}",
        prev,
        ts,
        actor.unwrap_or(0),
        action,
        tt.unwrap_or(""),
        tid.unwrap_or(""),
        detail.unwrap_or("")
    );
    let hash = hmac_sha256_b64(key, payload.as_bytes());
    conn.execute(
        "INSERT INTO audit_log (ts,actor_uid,action,target_type,target_id,ip,detail,prev_hash,hash) VALUES (?1,?2,?3,?4,?5,NULL,?6,?7,?8)",
        params![ts, actor, action, tt, tid, detail, prev, hash],
    )?;
    Ok(())
}

fn auth(state: &AppState, headers: &HeaderMap) -> ApiResult<AuthUser> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("缺少登录令牌"))?;
    let h = hmac_sha256_b64(&state.hmac, token.as_bytes());
    let conn = state.db.lock().unwrap();
    let row = conn
        .query_row(
            "SELECT s.uid, u.role, u.class_id FROM sessions s JOIN users u ON u.uid=s.uid WHERE s.token_hmac=?1 AND s.revoked=0 AND s.expires_at>?2 AND u.status='active'",
            params![h, now()],
            |r| Ok(AuthUser { uid: r.get(0)?, role: r.get(1)?, class_id: r.get(2)? }),
        )
        .optional()?;
    row.ok_or_else(|| AppError::unauthorized("登录已过期"))
}

fn notify(conn: &Connection, uid: i64, kind: &str, text: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO notifications (uid,kind,text,created_at) VALUES (?1,?2,?3,?4)",
        params![uid, kind, text, now()],
    )?;
    Ok(())
}

fn is_admin(a: &AuthUser) -> bool {
    a.role == "root" || a.role == "class_admin"
}

fn rate_allow(state: &AppState, uid: i64, kind: &str, limit: i64, window: i64) -> bool {
    let mut m = state.rate.lock().unwrap();
    let ts = now();
    let e = m.entry((uid, kind.to_string())).or_insert((ts, 0));
    if ts - e.0 >= window {
        *e = (ts, 0);
    }
    if e.1 >= limit {
        return false;
    }
    e.1 += 1;
    true
}

fn create_session(conn: &Connection, hmac_key: &[u8], uid: i64) -> rusqlite::Result<String> {
    let token = random_token();
    let h = hmac_sha256_b64(hmac_key, token.as_bytes());
    let id = uuid();
    conn.execute(
        "INSERT INTO sessions (id,uid,token_hmac,created_at,expires_at) VALUES (?1,?2,?3,?4,?5)",
        params![id, uid, h, now(), now() + 12 * 3600],
    )?;
    Ok(token)
}

fn load_classes(conn: &Connection) -> rusqlite::Result<Vec<ClassInfo>> {
    let mut stmt = conn.prepare(
        "SELECT c.id,c.name,c.grade,c.escrow_public_key,
         (SELECT COUNT(*) FROM roster_entries r WHERE r.class_id=c.id AND r.claimed_uid IS NULL AND r.status='normal')
         FROM classes c WHERE c.archived=0 ORDER BY c.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ClassInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            grade: r.get(2)?,
            escrow_public_key: r.get(3)?,
            unclaimed: r.get(4)?,
        })
    })?;
    rows.collect()
}

fn match_secret(
    input: &str,
    len: usize,
    salt: &str,
    params: &KdfParams,
    verify_b64: &str,
) -> Option<(usize, usize)> {
    let target = b64d(verify_b64).ok()?;
    let chars: Vec<char> = input.chars().collect();
    if len == 0 || chars.len() < len || chars.len() > 96 {
        return None;
    }
    for start in 0..=chars.len() - len {
        let window: String = chars[start..start + len].iter().collect();
        if let Ok((_, v)) = derive_keys(&window, salt, params) {
            if v.as_slice() == target.as_slice() {
                return Some((start, len));
            }
        }
    }
    None
}

fn build_login_response(
    conn: &Connection,
    uid: i64,
    matched_offset: usize,
    matched_len: usize,
    token: String,
) -> rusqlite::Result<LoginResponse> {
    let (handle, display_name, real_name, class_id, role, login_policy, public_key, avatar_file_id) = conn.query_row(
        "SELECT handle_display,display_name,real_name,class_id,role,login_policy,public_key,avatar_file_id FROM users WHERE uid=?1",
        params![uid],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
            ))
        },
    )?;
    let (pwd_salt, pwd_m, pwd_t, pwd_p, pin_salt, pin_m, pin_t, pin_p) = conn.query_row(
        "SELECT pwd_salt,pwd_m,pwd_t,pwd_p,pin_salt,pin_m,pin_t,pin_p FROM credentials WHERE uid=?1",
        params![uid],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<u32>>(5)?,
                r.get::<_, Option<u32>>(6)?,
                r.get::<_, Option<u32>>(7)?,
            ))
        },
    )?;
    let (own_priv, pin_own, user_priv_adm, user_priv_class) = conn.query_row(
        "SELECT own_priv,pin_own,user_priv_adm,user_priv_class FROM user_keys WHERE uid=?1",
        params![uid],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        },
    )?;
    let (profile_own, profile_pin, profile_adm, profile_class, profile_enc) = conn.query_row(
        "SELECT profile_own,profile_pin,profile_adm,profile_class,ciphertext FROM profiles_enc WHERE uid=?1",
        params![uid],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        },
    )?;
    let classes = load_classes(conn)?;
    Ok(LoginResponse {
        token,
        uid,
        handle,
        display_name,
        real_name,
        class_id,
        role,
        login_policy,
        public_key,
        avatar_file_id,
        server_time: now(),
        matched_offset,
        matched_len,
        pwd_salt,
        pwd_params: KdfParams { m: pwd_m, t: pwd_t, p: pwd_p },
        pin_salt,
        pin_params: match (pin_m, pin_t, pin_p) {
            (Some(m), Some(t), Some(p)) => Some(KdfParams { m, t, p }),
            _ => None,
        },
        own_priv,
        pin_own,
        user_priv_adm,
        user_priv_class,
        profile_own,
        profile_pin,
        profile_adm,
        profile_class,
        profile_enc,
        classes,
    })
}

fn ensure_class_group(conn: &Connection, class_id: i64) -> rusqlite::Result<i64> {
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM conversations WHERE kind='class_group' AND class_id=?1",
            params![class_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO conversations (kind,class_id,pair_key,current_key_version,created_at) VALUES ('class_group',?1,?2,0,?3)",
        params![class_id, format!("class:{class_id}"), now()],
    )?;
    Ok(conn.last_insert_rowid())
}

fn rotate_group_key(conn: &Connection, class_id: i64) -> ApiResult<()> {
    use rand::RngCore;
    let conv_id = ensure_class_group(conn, class_id)?;
    let current: i64 = conn.query_row(
        "SELECT current_key_version FROM conversations WHERE id=?1",
        params![conv_id],
        |r| r.get(0),
    )?;
    let next = current + 1;
    let root_pk: Option<String> = conn
        .query_row("SELECT public_key FROM root_state WHERE id=1", [], |r| r.get(0))
        .optional()?;
    let escrow_pk: Option<String> = conn
        .query_row(
            "SELECT escrow_public_key FROM classes WHERE id=?1",
            params![class_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let mut members: Vec<(i64, String)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT uid,public_key FROM users WHERE class_id=?1 AND status='active' AND role IN ('student','class_admin')",
        )?;
        let rows = stmt.query_map(params![class_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            members.push(row?);
        }
    }
    let mut gk = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut gk);
    for (uid, pk) in &members {
        let w = seal(pk, &gk)?;
        conn.execute(
            "INSERT OR REPLACE INTO conversation_keys (conv_id,key_version,recipient_uid,wrapped_key,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![conv_id, next, uid, w, now()],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO conversation_members (conv_id,uid,last_read,joined_at) VALUES (?1,?2,0,?3)",
            params![conv_id, uid, now()],
        )?;
    }
    if let Some(pk) = root_pk {
        let w = seal(&pk, &gk)?;
        conn.execute(
            "INSERT OR REPLACE INTO conversation_keys (conv_id,key_version,recipient_uid,wrapped_key,created_at) VALUES (?1,?2,0,?3,?4)",
            params![conv_id, next, w, now()],
        )?;
    }
    if let Some(pk) = escrow_pk {
        let w = seal(&pk, &gk)?;
        conn.execute(
            "INSERT OR REPLACE INTO conversation_keys (conv_id,key_version,recipient_uid,wrapped_key,created_at) VALUES (?1,?2,-1,?3,?4)",
            params![conv_id, next, w, now()],
        )?;
    }
    gk.fill(0);
    conn.execute(
        "UPDATE conversations SET current_key_version=?1 WHERE id=?2",
        params![next, conv_id],
    )?;
    Ok(())
}

// ---------------- handlers ----------------

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"status":"ok","version":state.version.as_str(),"protocol":PROTOCOL_VERSION,"server_time":now()}))
}

async fn bootstrap(State(state): State<AppState>) -> ApiResult<Json<BootstrapResponse>> {
    let conn = state.db.lock().unwrap();
    let root_count: i64 = conn.query_row("SELECT COUNT(*) FROM root_state", [], |r| r.get(0))?;
    let setup_required = root_count == 0;
    let root_pk: Option<String> = conn
        .query_row("SELECT public_key FROM root_state WHERE id=1", [], |r| r.get(0))
        .optional()?;
    let classes = load_classes(&conn)?;
    Ok(Json(BootstrapResponse {
        setup_required,
        setup_token_required: setup_required,
        school_name: get_setting(&conn, "school_name")?.unwrap_or_else(|| "杨村一中".into()),
        server_name: get_setting(&conn, "server_name")?.unwrap_or_else(|| "liaisonyy".into()),
        version: state.version.to_string(),
        protocol: PROTOCOL_VERSION,
        server_time: now(),
        root_public_key: root_pk,
        kdf_params: KdfParams::default(),
        classes,
    }))
}

async fn setup(State(state): State<AppState>, Json(req): Json<SetupRequest>) -> ApiResult<Json<Value>> {
    let token = state.setup_token.lock().unwrap().clone();
    if token.is_empty() {
        return Err(AppError::conflict("服务端已初始化"));
    }
    if req.setup_token != token {
        return Err(AppError::forbidden("初始化口令不正确"));
    }
    validate_handle(&req.handle)?;
    validate_password(&req.password)?;
    let (_, verify) = derive_keys(&req.password, &req.pwd_salt, &req.pwd_params)?;
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    let root_count: i64 = tx.query_row("SELECT COUNT(*) FROM root_state", [], |r| r.get(0))?;
    if root_count > 0 {
        return Err(AppError::conflict("服务端已初始化"));
    }
    tx.execute(
        "INSERT INTO users (handle_lower,handle_display,display_name,real_name,class_id,roster_id,role,status,login_policy,public_key,created_at) VALUES (?1,?2,?3,?3,NULL,NULL,'root','active','password_only',?4,?5)",
        params![req.handle.to_lowercase(), req.handle, req.display_name, req.public_key, now()],
    )?;
    let uid = tx.last_insert_rowid();
    let pwd_len = req.password.chars().count();
    tx.execute(
        "INSERT INTO credentials (uid,pwd_salt,pwd_m,pwd_t,pwd_p,pwd_verify,pwd_len) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![uid, req.pwd_salt, req.pwd_params.m, req.pwd_params.t, req.pwd_params.p, b64e(&verify), pwd_len],
    )?;
    tx.execute(
        "INSERT INTO user_keys (uid,own_priv,pin_own,user_priv_adm,user_priv_class) VALUES (?1,?2,NULL,'','')",
        params![uid, req.root_own],
    )?;
    tx.execute(
        "INSERT INTO profiles_enc (uid,ciphertext,profile_own,profile_pin,profile_adm,profile_class) VALUES (?1,'','',NULL,'','')",
        params![uid],
    )?;
    tx.execute(
        "INSERT INTO root_state (id,public_key,root_own,root_recovery,root_verify,root_salt,root_m,root_t,root_p,root_len) VALUES (1,?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![req.public_key, req.root_own, req.root_recovery, b64e(&verify), req.pwd_salt, req.pwd_params.m, req.pwd_params.t, req.pwd_params.p, pwd_len],
    )?;
    set_setting(&tx, "setup_token", "")?;
    audit(&tx, &state.hmac, Some(uid), "server.setup", None, None, None)?;
    tx.commit()?;
    *state.setup_token.lock().unwrap() = String::new();
    Ok(Json(json!({"ok":true,"uid":uid})))
}

async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> ApiResult<Json<LoginResponse>> {
    validate_handle(&req.handle)?;
    validate_password(&req.password)?;
    if req.login_policy == "pin_allowed" {
        let pin = req.pin.as_deref().ok_or_else(|| AppError::bad("缺少 PIN"))?;
        validate_pin(pin)?;
    }
    let (_, pwd_verify) = derive_keys(&req.password, &req.pwd_salt, &req.pwd_params)?;
    let pin_derived = if req.login_policy == "pin_allowed" {
        let pin = req.pin.as_deref().unwrap();
        let salt = req.pin_salt.as_deref().ok_or_else(|| AppError::bad("缺少 PIN salt"))?;
        let params = req.pin_params.as_ref().ok_or_else(|| AppError::bad("缺少 PIN 参数"))?;
        let (_, v) = derive_keys(pin, salt, params)?;
        Some((v, pin.chars().count()))
    } else {
        None
    };
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    let (roster_class, roster_name, claimed, status): (i64, String, Option<i64>, String) = tx
        .query_row(
            "SELECT class_id,real_name,claimed_uid,status FROM roster_entries WHERE id=?1",
            params![req.roster_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("名单条目不存在"))?;
    if roster_class != req.class_id || roster_name != req.real_name || claimed.is_some() || status != "normal" {
        return Err(AppError::conflict("该名单条目不可注册"));
    }
    let escrow_pk: Option<String> = tx
        .query_row(
            "SELECT escrow_public_key FROM classes WHERE id=?1 AND archived=0",
            params![req.class_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if escrow_pk.is_none() {
        return Err(AppError::conflict("班级尚未初始化密钥"));
    }
    let exists: i64 = tx.query_row(
        "SELECT COUNT(*) FROM users WHERE handle_lower=?1",
        params![req.handle.to_lowercase()],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Err(AppError::conflict("YY 号已被占用"));
    }
    tx.execute(
        "INSERT INTO users (handle_lower,handle_display,display_name,real_name,class_id,roster_id,role,status,login_policy,public_key,created_at) VALUES (?1,?2,?3,?4,?5,?6,'student','active',?7,?8,?9)",
        params![req.handle.to_lowercase(), req.handle, req.display_name, req.real_name, req.class_id, req.roster_id, req.login_policy, req.public_key, now()],
    )?;
    let uid = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO credentials (uid,pwd_salt,pwd_m,pwd_t,pwd_p,pwd_verify,pwd_len,pin_salt,pin_m,pin_t,pin_p,pin_verify,pin_len) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            uid,
            req.pwd_salt,
            req.pwd_params.m,
            req.pwd_params.t,
            req.pwd_params.p,
            b64e(&pwd_verify),
            req.password.chars().count(),
            req.pin_salt,
            req.pin_params.as_ref().map(|p| p.m),
            req.pin_params.as_ref().map(|p| p.t),
            req.pin_params.as_ref().map(|p| p.p),
            pin_derived.as_ref().map(|(v, _)| b64e(v)),
            pin_derived.as_ref().map(|(_, n)| *n as i64),
        ],
    )?;
    tx.execute(
        "INSERT INTO user_keys (uid,own_priv,pin_own,user_priv_adm,user_priv_class) VALUES (?1,?2,?3,?4,?5)",
        params![uid, req.own_priv, req.pin_own, req.user_priv_adm, req.user_priv_class],
    )?;
    tx.execute(
        "INSERT INTO profiles_enc (uid,ciphertext,profile_own,profile_pin,profile_adm,profile_class) VALUES (?1,'',?2,?3,?4,?5)",
        params![uid, req.profile_own, req.profile_pin, req.profile_adm, req.profile_class],
    )?;
    tx.execute(
        "UPDATE roster_entries SET claimed_uid=?1 WHERE id=?2",
        params![uid, req.roster_id],
    )?;
    rotate_group_key(&tx, req.class_id)?;
    let token = create_session(&tx, &state.hmac, uid)?;
    audit(&tx, &state.hmac, Some(uid), "account.register", Some("uid"), Some(&uid.to_string()), None)?;
    tx.commit()?;
    let resp = build_login_response(&conn, uid, 0, req.password.chars().count(), token)?;
    Ok(Json(resp))
}

async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> ApiResult<Json<LoginResponse>> {
    let conn = state.db.lock().unwrap();
    let key = req.uid_or_handle.trim();
    let uid: Option<i64> = if key.chars().all(|c| c.is_ascii_digit()) {
        conn.query_row("SELECT uid FROM users WHERE uid=?1", params![key.parse::<i64>().unwrap_or(-1)], |r| r.get(0)).optional()?
    } else {
        conn.query_row("SELECT uid FROM users WHERE handle_lower=?1", params![key.to_lowercase()], |r| r.get(0)).optional()?
    };
    let uid = uid.ok_or_else(|| AppError::unauthorized("账号不存在或凭据错误"))?;
    let (status, policy, locked_until, failed_count): (String, String, i64, i64) = conn.query_row(
        "SELECT u.status,u.login_policy,c.locked_until,c.failed_count FROM users u JOIN credentials c ON c.uid=u.uid WHERE u.uid=?1",
        params![uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    if status != "active" {
        return Err(AppError::forbidden("账号已被禁用"));
    }
    if locked_until > now() {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "locked", "尝试过多，请稍后再试"));
    }
    let kind = req.kind.as_str();
    if kind == "pin" && policy != "pin_allowed" {
        return Err(AppError::forbidden("该账号未开启 PIN 登录"));
    }
    let (salt, m, t, p, verify, len): (String, u32, u32, u32, String, i64) = if kind == "pin" {
        let row = conn.query_row(
            "SELECT pin_salt,pin_m,pin_t,pin_p,pin_verify,pin_len FROM credentials WHERE uid=?1",
            params![uid],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<u32>>(1)?, r.get::<_, Option<u32>>(2)?, r.get::<_, Option<u32>>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?)),
        )?;
        match row {
            (Some(s), Some(m), Some(t), Some(p), Some(v), Some(l)) => (s, m, t, p, v, l),
            _ => return Err(AppError::forbidden("该账号未设置 PIN")),
        }
    } else {
        conn.query_row(
            "SELECT pwd_salt,pwd_m,pwd_t,pwd_p,pwd_verify,pwd_len FROM credentials WHERE uid=?1",
            params![uid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?
    };
    let params_kdf = KdfParams { m, t, p };
    let matched = match_secret(&req.input, len as usize, &salt, &params_kdf, &verify);
    if matched.is_none() {
        let n = failed_count + 1;
        let lock = if n >= 10 { 300 } else if n >= 5 { 60 } else { 0 };
        conn.execute(
            "UPDATE credentials SET failed_count=?1, locked_until=?2 WHERE uid=?3",
            params![n, now() + lock, uid],
        )?;
        audit(&conn, &state.hmac, Some(uid), "auth.login.fail", Some("uid"), Some(&uid.to_string()), None)?;
        return Err(AppError::unauthorized("账号或密码错误"));
    }
    let (off, l) = matched.unwrap();
    conn.execute(
        "UPDATE credentials SET failed_count=0, locked_until=0 WHERE uid=?1",
        params![uid],
    )?;
    let token = create_session(&conn, &state.hmac, uid)?;
    audit(&conn, &state.hmac, Some(uid), "auth.login", Some("uid"), Some(&uid.to_string()), None)?;
    let resp = build_login_response(&conn, uid, off, l, token)?;
    Ok(Json(resp))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("缺少登录令牌"))?;
    let h = hmac_sha256_b64(&state.hmac, token.as_bytes());
    let conn = state.db.lock().unwrap();
    conn.execute("UPDATE sessions SET revoked=1 WHERE token_hmac=?1", params![h])?;
    Ok(Json(json!({"ok":true})))
}

async fn list_classes(State(state): State<AppState>) -> ApiResult<Json<Vec<ClassInfo>>> {
    let conn = state.db.lock().unwrap();
    Ok(Json(load_classes(&conn)?))
}

async fn list_roster(State(state): State<AppState>, Path(class_id): Path<i64>) -> ApiResult<Json<Vec<RosterEntry>>> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,real_name,student_no,claimed_uid FROM roster_entries WHERE class_id=?1 AND status='normal' ORDER BY id",
    )?;
    let rows = stmt.query_map(params![class_id], |r| {
        Ok(RosterEntry {
            id: r.get(0)?,
            real_name: r.get(1)?,
            student_no: r.get(2)?,
            claimed: r.get::<_, Option<i64>>(3)?.is_some(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn list_accounts(State(state): State<AppState>, Path(class_id): Path<i64>) -> ApiResult<Json<Vec<AccountPublic>>> {
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT uid,handle_display,display_name,class_id,role,login_policy,public_key,avatar_file_id FROM users WHERE class_id=?1 AND status='active' ORDER BY uid",
    )?;
    let rows = stmt.query_map(params![class_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, Option<i64>>(7)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (uid, handle, display_name, class_id, role, login_policy, public_key, avatar_file_id) = row?;
        let unread: i64 = conn.query_row(
            "SELECT COALESCE(SUM((SELECT COUNT(*) FROM messages m WHERE m.conv_id=cm.conv_id AND m.id>cm.last_read AND m.sender_uid!=?1)),0) FROM conversation_members cm WHERE cm.uid=?1",
            params![uid],
            |r| r.get(0),
        )?;
        out.push(AccountPublic { uid, handle, display_name, class_id, role, login_policy, public_key, avatar_file_id, unread });
    }
    Ok(Json(out))
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<MeResponse>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let (handle, display_name, real_name, class_id, role, login_policy, public_key, avatar_file_id) = conn.query_row(
        "SELECT handle_display,display_name,real_name,class_id,role,login_policy,public_key,avatar_file_id FROM users WHERE uid=?1",
        params![a.uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
    )?;
    Ok(Json(MeResponse { uid: a.uid, handle, display_name, real_name, class_id, role, login_policy, public_key, avatar_file_id }))
}

async fn update_me(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<UpdateMeRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    if let Some(name) = req.display_name {
        let n = name.chars().count();
        if !(1..=20).contains(&n) {
            return Err(AppError::bad("用户名需 1-20 字"));
        }
        conn.execute("UPDATE users SET display_name=?1 WHERE uid=?2", params![name, a.uid])?;
    }
    if let Some(enc) = req.profile_enc {
        conn.execute("UPDATE profiles_enc SET ciphertext=?1 WHERE uid=?2", params![enc, a.uid])?;
    }
    audit(&conn, &state.hmac, Some(a.uid), "account.update", Some("uid"), Some(&a.uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn get_user(State(state): State<AppState>, Path(uid): Path<i64>) -> ApiResult<Json<AccountPublic>> {
    let conn = state.db.lock().unwrap();
    let (handle, display_name, class_id, role, login_policy, public_key, avatar_file_id) = conn.query_row(
        "SELECT handle_display,display_name,class_id,role,login_policy,public_key,avatar_file_id FROM users WHERE uid=?1",
        params![uid],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, Option<i64>>(6)?)),
    ).optional()?.ok_or_else(|| AppError::not_found("用户不存在"))?;
    Ok(Json(AccountPublic { uid, handle, display_name, class_id: class_id.unwrap_or(0), role, login_policy, public_key, avatar_file_id, unread: 0 }))
}

fn validate_kdf(p: &KdfParams) -> ApiResult<()> {
    if !(8192..=1_048_576).contains(&p.m) || !(1..=10).contains(&p.t) || !(1..=8).contains(&p.p) {
        return Err(AppError::bad("KDF 参数不正确"));
    }
    Ok(())
}

fn verify_password(conn: &Connection, uid: i64, input: &str) -> ApiResult<()> {
    let (salt, m, t, p, verify, len): (String, u32, u32, u32, String, i64) = conn.query_row(
        "SELECT pwd_salt,pwd_m,pwd_t,pwd_p,pwd_verify,pwd_len FROM credentials WHERE uid=?1",
        params![uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
    )?;
    let kdf = KdfParams { m, t, p };
    if match_secret(input, len as usize, &salt, &kdf, &verify).is_none() {
        return Err(AppError::unauthorized("密码错误"));
    }
    Ok(())
}

fn revoke_other_sessions(conn: &Connection, hmac_key: &[u8], uid: i64, headers: &HeaderMap) -> ApiResult<()> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("缺少登录令牌"))?;
    let cur = hmac_sha256_b64(hmac_key, token.as_bytes());
    conn.execute(
        "UPDATE sessions SET revoked=1 WHERE uid=?1 AND token_hmac<>?2",
        params![uid, cur],
    )?;
    Ok(())
}

fn public_profile(conn: &Connection, viewer: i64, target: i64) -> ApiResult<Option<PublicProfile>> {
    let row = conn
        .query_row(
            "SELECT u.uid,u.handle_display,u.display_name,u.real_name,u.class_id,c.name,u.role,u.avatar_file_id,
                    p.background_file_id,COALESCE(p.signature,''),COALESCE(p.bio,''),p.gender,p.birthday,p.socials_json
             FROM users u LEFT JOIN classes c ON c.id=u.class_id LEFT JOIN public_profiles p ON p.uid=u.uid
             WHERE u.uid=?1 AND u.status='active'",
            params![target],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                    r.get::<_, Option<String>>(11)?,
                    r.get::<_, Option<String>>(12)?,
                    r.get::<_, Option<String>>(13)?,
                ))
            },
        )
        .optional()?;
    let Some((uid, handle, display_name, real_name, class_id, class_name, role, avatar_file_id, background_file_id, signature, bio, gender, birthday, socials_json)) = row else {
        return Ok(None);
    };
    let is_friend = viewer != target
        && conn.query_row(
            "SELECT COUNT(*) FROM friends WHERE uid=?1 AND friend_uid=?2",
            params![viewer, target],
            |r| r.get::<_, i64>(0),
        )? > 0;
    let is_blocked = viewer != target
        && conn.query_row(
            "SELECT COUNT(*) FROM blocks WHERE uid=?1 AND blocked_uid=?2",
            params![viewer, target],
            |r| r.get::<_, i64>(0),
        )? > 0;
    Ok(Some(PublicProfile {
        uid,
        handle,
        display_name,
        real_name,
        class_id,
        class_name,
        role,
        avatar_file_id,
        background_file_id,
        signature,
        bio,
        gender,
        birthday,
        socials_json,
        is_friend,
        is_blocked,
    }))
}

async fn get_my_profile(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<MyProfileResponse>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let row = conn.query_row(
        "SELECT u.uid,u.handle_display,u.display_name,u.real_name,u.class_id,u.role,u.login_policy,u.avatar_file_id,
                p.background_file_id,COALESCE(p.signature,''),COALESCE(p.bio,''),p.gender,p.birthday,p.socials_json,
                COALESCE(pe.ciphertext,'')
         FROM users u LEFT JOIN public_profiles p ON p.uid=u.uid LEFT JOIN profiles_enc pe ON pe.uid=u.uid
         WHERE u.uid=?1",
        params![a.uid],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, Option<String>>(12)?,
                r.get::<_, Option<String>>(13)?,
                r.get::<_, String>(14)?,
            ))
        },
    )?;
    Ok(Json(MyProfileResponse {
        uid: row.0,
        handle: row.1,
        display_name: row.2,
        real_name: row.3,
        class_id: row.4,
        role: row.5,
        login_policy: row.6,
        avatar_file_id: row.7,
        background_file_id: row.8,
        signature: row.9,
        bio: row.10,
        gender: row.11,
        birthday: row.12,
        socials_json: row.13,
        profile_enc: row.14,
    }))
}

async fn update_profile(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<UpdateProfileRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let name = req.display_name.trim();
    let n = name.chars().count();
    if !(1..=20).contains(&n) {
        return Err(AppError::bad("用户名需 1-20 字"));
    }
    if req.signature.chars().count() > 100 {
        return Err(AppError::bad("个性签名最多 100 字"));
    }
    if req.bio.chars().count() > 1000 {
        return Err(AppError::bad("个人简介最多 1000 字"));
    }
    if req.public_socials.len() > 2000 {
        return Err(AppError::bad("社交账号信息过长"));
    }
    if req.profile_enc.len() > 65536 {
        return Err(AppError::bad("资料数据过大"));
    }
    let gender = if req.public_gender.trim().is_empty() { None } else { Some(req.public_gender.trim().to_string()) };
    let birthday = if req.public_birthday.trim().is_empty() { None } else { Some(req.public_birthday.trim().to_string()) };
    let socials = if req.public_socials.trim().is_empty() { None } else { Some(req.public_socials) };
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE users SET display_name=?1 WHERE uid=?2", params![name, a.uid])?;
    tx.execute(
        "INSERT INTO public_profiles (uid,signature,bio,gender,birthday,socials_json) VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(uid) DO UPDATE SET signature=excluded.signature,bio=excluded.bio,gender=excluded.gender,birthday=excluded.birthday,socials_json=excluded.socials_json",
        params![a.uid, req.signature, req.bio, gender, birthday, socials],
    )?;
    tx.execute("UPDATE profiles_enc SET ciphertext=?1 WHERE uid=?2", params![req.profile_enc, a.uid])?;
    if let Some(w) = req.profile_own.filter(|s| !s.is_empty()) {
        if w.len() > 4096 {
            return Err(AppError::bad("资料密钥过长"));
        }
        tx.execute("UPDATE profiles_enc SET profile_own=?1 WHERE uid=?2", params![w, a.uid])?;
    }
    audit(&tx, &state.hmac, Some(a.uid), "account.profile.update", Some("uid"), Some(&a.uid.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}

async fn set_profile_key(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<SetProfileKeyRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if req.profile_own.is_empty() || req.profile_own.len() > 4096 {
        return Err(AppError::bad("资料密钥格式不正确"));
    }
    let conn = state.db.lock().unwrap();
    conn.execute("UPDATE profiles_enc SET profile_own=?1 WHERE uid=?2", params![req.profile_own, a.uid])?;
    audit(&conn, &state.hmac, Some(a.uid), "account.profile.key", Some("uid"), Some(&a.uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn rekey_class(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<RekeyClassRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if req.user_priv_class.is_empty()
        || req.user_priv_class.len() > 4096
        || req.profile_class.is_empty()
        || req.profile_class.len() > 4096
    {
        return Err(AppError::bad("班级密钥格式不正确"));
    }
    let conn = state.db.lock().unwrap();
    let class_id: Option<i64> = conn
        .query_row("SELECT class_id FROM users WHERE uid=?1", params![a.uid], |r| r.get(0))
        .optional()?
        .flatten();
    let cid = class_id.ok_or_else(|| AppError::bad("你还没有班级"))?;
    let has_pk: Option<String> = conn
        .query_row("SELECT escrow_public_key FROM classes WHERE id=?1", params![cid], |r| r.get(0))
        .optional()?
        .flatten();
    if has_pk.is_none() {
        return Err(AppError::bad("班级密钥未初始化"));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE user_keys SET user_priv_class=?1 WHERE uid=?2",
        params![req.user_priv_class, a.uid],
    )?;
    tx.execute(
        "UPDATE profiles_enc SET profile_class=?1 WHERE uid=?2",
        params![req.profile_class, a.uid],
    )?;
    audit(&tx, &state.hmac, Some(a.uid), "account.class_rekey", Some("class"), Some(&cid.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}

async fn get_user_profile(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>) -> ApiResult<Json<PublicProfile>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let p = public_profile(&conn, a.uid, uid)?.ok_or_else(|| AppError::not_found("用户不存在"))?;
    Ok(Json(p))
}

async fn lookup_user(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<PublicProfile>> {
    let a = auth(&state, &headers)?;
    let key = q.get("q").map(|s| s.trim()).unwrap_or("");
    if key.is_empty() {
        return Err(AppError::bad("请输入 YY 号或 UID"));
    }
    let conn = state.db.lock().unwrap();
    let uid: Option<i64> = if key.chars().all(|c| c.is_ascii_digit()) {
        conn.query_row(
            "SELECT uid FROM users WHERE uid=?1 AND status='active'",
            params![key.parse::<i64>().unwrap_or(-1)],
            |r| r.get(0),
        )
        .optional()?
    } else {
        conn.query_row(
            "SELECT uid FROM users WHERE handle_lower=?1 AND status='active'",
            params![key.to_lowercase()],
            |r| r.get(0),
        )
        .optional()?
    };
    let uid = uid.ok_or_else(|| AppError::not_found("未找到该用户"))?;
    let p = public_profile(&conn, a.uid, uid)?.ok_or_else(|| AppError::not_found("未找到该用户"))?;
    Ok(Json(p))
}

async fn upload_avatar(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<AvatarRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let kind = req.kind.as_deref().unwrap_or("avatar");
    if kind != "avatar" && kind != "background" {
        return Err(AppError::bad("未知的图片类型"));
    }
    let raw = req.data.split(',').last().unwrap_or(&req.data).trim();
    let bytes = b64d(raw).map_err(|_| AppError::bad("图片数据无效"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::bad("图片过大（最大 8 MB）"));
    }
    let img = image::load_from_memory(&bytes).map_err(|_| AppError::bad("图片格式不支持"))?;
    let (w, h) = if kind == "background" { (1280u32, 720u32) } else { (256u32, 256u32) };
    let img = img.resize_to_fill(w, h, image::imageops::FilterType::Triangle);
    let mut cursor = std::io::Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| AppError::bad(format!("图片处理失败: {e}")))?;
    let png = cursor.into_inner();
    if png.len() > 4 * 1024 * 1024 {
        return Err(AppError::bad("图片处理结果过大"));
    }
    let sha = sha256_b64(&png);
    let enc = aead_encrypt(state.media_key.as_ref(), &png, b"liaisonyy-media-v1");
    let fname = format!("{}.png", uuid());
    let media_dir = state.data.join("media");
    std::fs::create_dir_all(&media_dir)?;
    std::fs::write(media_dir.join(&fname), &enc)?;
    let conn = state.db.lock().unwrap();
    let old: Option<i64> = conn
        .query_row(
            "SELECT CASE WHEN ?2='background' THEN p.background_file_id ELSE u.avatar_file_id END
             FROM users u LEFT JOIN public_profiles p ON p.uid=u.uid WHERE u.uid=?1",
            params![a.uid, kind],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO files (owner_uid,sha256,size,mime,path,created_at) VALUES (?1,?2,?3,'image/png',?4,?5)",
        params![a.uid, sha, png.len() as i64, fname, now()],
    )?;
    let file_id = tx.last_insert_rowid();
    if kind == "background" {
        tx.execute(
            "INSERT INTO public_profiles (uid,background_file_id) VALUES (?1,?2)
             ON CONFLICT(uid) DO UPDATE SET background_file_id=excluded.background_file_id",
            params![a.uid, file_id],
        )?;
    } else {
        tx.execute("UPDATE users SET avatar_file_id=?1 WHERE uid=?2", params![file_id, a.uid])?;
        tx.execute(
            "INSERT INTO public_profiles (uid,avatar_file_id) VALUES (?1,?2)
             ON CONFLICT(uid) DO UPDATE SET avatar_file_id=excluded.avatar_file_id",
            params![a.uid, file_id],
        )?;
    }
    audit(&tx, &state.hmac, Some(a.uid), if kind == "background" { "account.background.update" } else { "account.avatar.update" }, Some("uid"), Some(&a.uid.to_string()), None)?;
    tx.commit()?;
    if let Some(old_id) = old {
        if old_id != file_id {
            if let Ok(path) = conn.query_row("SELECT path FROM files WHERE id=?1", params![old_id], |r| r.get::<_, String>(0)) {
                let _ = std::fs::remove_file(media_dir.join(path));
                let _ = conn.execute("DELETE FROM files WHERE id=?1", params![old_id]);
            }
        }
    }
    Ok(Json(json!({"file_id": file_id})))
}

async fn get_file(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Response> {
    let (path, mime): (String, String) = {
        let conn = state.db.lock().unwrap();
        conn.query_row("SELECT path,mime FROM files WHERE id=?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| AppError::not_found("文件不存在"))?
    };
    if path.len() > 64 || !path.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(AppError::not_found("文件不存在"));
    }
    let enc = std::fs::read(state.data.join("media").join(&path)).map_err(|_| AppError::not_found("文件不存在"))?;
    let plain = aead_decrypt(state.media_key.as_ref(), &enc, b"liaisonyy-media-v1")?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(plain))
        .map_err(|e| AppError::internal(e.to_string()))
}

async fn change_password(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<ChangePasswordRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    validate_password(&req.new_password)?;
    validate_kdf(&req.pwd_params)?;
    let conn = state.db.lock().unwrap();
    verify_password(&conn, a.uid, &req.old_input)?;
    let (kek, new_verify) = derive_keys(&req.new_password, &req.pwd_salt, &req.pwd_params)
        .map_err(|_| AppError::bad("密码参数无效"))?;
    if unwrap_key(&kek, &req.own_priv).is_err() || unwrap_key(&kek, &req.profile_own).is_err() {
        return Err(AppError::bad("密钥包裹不正确"));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE credentials SET pwd_salt=?1,pwd_m=?2,pwd_t=?3,pwd_p=?4,pwd_verify=?5,pwd_len=?6,failed_count=0,locked_until=0 WHERE uid=?7",
        params![
            req.pwd_salt,
            req.pwd_params.m,
            req.pwd_params.t,
            req.pwd_params.p,
            b64e(&new_verify),
            req.new_password.chars().count() as i64,
            a.uid
        ],
    )?;
    tx.execute("UPDATE user_keys SET own_priv=?1 WHERE uid=?2", params![req.own_priv, a.uid])?;
    tx.execute("UPDATE profiles_enc SET profile_own=?1 WHERE uid=?2", params![req.profile_own, a.uid])?;
    revoke_other_sessions(&tx, &state.hmac, a.uid, &headers)?;
    audit(&tx, &state.hmac, Some(a.uid), "account.password.change", Some("uid"), Some(&a.uid.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}

async fn set_pin(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<SetPinRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    validate_pin(&req.pin)?;
    validate_kdf(&req.pin_params)?;
    let conn = state.db.lock().unwrap();
    verify_password(&conn, a.uid, &req.password_input)?;
    let (kek, pin_verify) = derive_keys(&req.pin, &req.pin_salt, &req.pin_params)
        .map_err(|_| AppError::bad("PIN 参数无效"))?;
    if unwrap_key(&kek, &req.pin_own).is_err() || unwrap_key(&kek, &req.profile_pin).is_err() {
        return Err(AppError::bad("密钥包裹不正确"));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE credentials SET pin_salt=?1,pin_m=?2,pin_t=?3,pin_p=?4,pin_verify=?5,pin_len=?6 WHERE uid=?7",
        params![
            req.pin_salt,
            req.pin_params.m,
            req.pin_params.t,
            req.pin_params.p,
            b64e(&pin_verify),
            req.pin.chars().count() as i64,
            a.uid
        ],
    )?;
    tx.execute("UPDATE user_keys SET pin_own=?1 WHERE uid=?2", params![req.pin_own, a.uid])?;
    tx.execute("UPDATE profiles_enc SET profile_pin=?1 WHERE uid=?2", params![req.profile_pin, a.uid])?;
    tx.execute("UPDATE users SET login_policy='pin_allowed' WHERE uid=?1", params![a.uid])?;
    revoke_other_sessions(&tx, &state.hmac, a.uid, &headers)?;
    audit(&tx, &state.hmac, Some(a.uid), "account.pin.set", Some("uid"), Some(&a.uid.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}

async fn set_login_policy(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<SetLoginPolicyRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    verify_password(&conn, a.uid, &req.password_input)?;
    match req.policy.as_str() {
        "pin_allowed" => {
            let pin = req.pin.as_deref().ok_or_else(|| AppError::bad("请设置 PIN"))?;
            validate_pin(pin)?;
            let params = req.pin_params.as_ref().ok_or_else(|| AppError::bad("PIN 参数缺失"))?;
            validate_kdf(params)?;
            let salt = req.pin_salt.as_deref().ok_or_else(|| AppError::bad("PIN 参数缺失"))?;
            let pin_own = req.pin_own.as_deref().ok_or_else(|| AppError::bad("PIN 密钥缺失"))?;
            let profile_pin = req.profile_pin.as_deref().ok_or_else(|| AppError::bad("PIN 密钥缺失"))?;
            let (kek, pin_verify) = derive_keys(pin, salt, params).map_err(|_| AppError::bad("PIN 参数无效"))?;
            if unwrap_key(&kek, pin_own).is_err() || unwrap_key(&kek, profile_pin).is_err() {
                return Err(AppError::bad("密钥包裹不正确"));
            }
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "UPDATE credentials SET pin_salt=?1,pin_m=?2,pin_t=?3,pin_p=?4,pin_verify=?5,pin_len=?6 WHERE uid=?7",
                params![salt, params.m, params.t, params.p, b64e(&pin_verify), pin.chars().count() as i64, a.uid],
            )?;
            tx.execute("UPDATE user_keys SET pin_own=?1 WHERE uid=?2", params![pin_own, a.uid])?;
            tx.execute("UPDATE profiles_enc SET profile_pin=?1 WHERE uid=?2", params![profile_pin, a.uid])?;
            tx.execute("UPDATE users SET login_policy='pin_allowed' WHERE uid=?1", params![a.uid])?;
            revoke_other_sessions(&tx, &state.hmac, a.uid, &headers)?;
            audit(&tx, &state.hmac, Some(a.uid), "account.login_policy", Some("uid"), Some(&a.uid.to_string()), Some("pin_allowed"))?;
            tx.commit()?;
        }
        "password_only" => {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "UPDATE credentials SET pin_salt=NULL,pin_m=NULL,pin_t=NULL,pin_p=NULL,pin_verify=NULL,pin_len=NULL WHERE uid=?1",
                params![a.uid],
            )?;
            tx.execute("UPDATE user_keys SET pin_own=NULL WHERE uid=?1", params![a.uid])?;
            tx.execute("UPDATE profiles_enc SET profile_pin=NULL WHERE uid=?1", params![a.uid])?;
            tx.execute("UPDATE users SET login_policy='password_only' WHERE uid=?1", params![a.uid])?;
            revoke_other_sessions(&tx, &state.hmac, a.uid, &headers)?;
            audit(&tx, &state.hmac, Some(a.uid), "account.login_policy", Some("uid"), Some(&a.uid.to_string()), Some("password_only"))?;
            tx.commit()?;
        }
        _ => return Err(AppError::bad("未知的登录方式")),
    }
    Ok(Json(json!({"ok":true})))
}

async fn delete_friend(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let n = conn.execute(
        "DELETE FROM friends WHERE (uid=?1 AND friend_uid=?2) OR (uid=?2 AND friend_uid=?1)",
        params![a.uid, uid],
    )?;
    if n == 0 {
        return Err(AppError::not_found("不是好友"));
    }
    audit(&conn, &state.hmac, Some(a.uid), "friend.delete", Some("uid"), Some(&uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn block_user(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<BlockRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if req.uid == a.uid {
        return Err(AppError::bad("不能拉黑自己"));
    }
    let conn = state.db.lock().unwrap();
    let exists: Option<i64> = conn
        .query_row("SELECT uid FROM users WHERE uid=?1 AND status='active'", params![req.uid], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(AppError::not_found("用户不存在"));
    }
    conn.execute(
        "INSERT OR IGNORE INTO blocks (uid,blocked_uid,created_at) VALUES (?1,?2,?3)",
        params![a.uid, req.uid, now()],
    )?;
    audit(&conn, &state.hmac, Some(a.uid), "user.block", Some("uid"), Some(&req.uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn unblock_user(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    conn.execute("DELETE FROM blocks WHERE uid=?1 AND blocked_uid=?2", params![a.uid, uid])?;
    audit(&conn, &state.hmac, Some(a.uid), "user.unblock", Some("uid"), Some(&uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn list_blocks(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<PublicProfile>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT blocked_uid FROM blocks WHERE uid=?1 ORDER BY created_at DESC")?;
    let uids = stmt
        .query_map(params![a.uid], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for uid in uids {
        if let Some(p) = public_profile(&conn, a.uid, uid)? {
            out.push(p);
        }
    }
    Ok(Json(out))
}

struct ReportTarget {
    uid: Option<i64>,
    name: Option<String>,
    message_id: Option<i64>,
    conv_id: Option<i64>,
    class_id: Option<i64>,
}

fn report_target(conn: &Connection, target_type: &str, target_id: i64) -> ApiResult<Option<ReportTarget>> {
    if target_type == "user" {
        let r = conn
            .query_row(
                "SELECT uid,display_name,class_id FROM users WHERE uid=?1",
                params![target_id],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?)),
            )
            .optional()?;
        Ok(r.map(|(uid, name, class_id)| ReportTarget {
            uid: Some(uid),
            name: Some(name),
            message_id: None,
            conv_id: None,
            class_id,
        }))
    } else if target_type == "message" {
        let r = conn
            .query_row(
                "SELECT m.id,m.conv_id,m.sender_uid,u.display_name,u.class_id
                 FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE m.id=?1",
                params![target_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()?;
        Ok(r.map(|(mid, cid, uid, name, class_id)| ReportTarget {
            uid: Some(uid),
            name: Some(name),
            message_id: Some(mid),
            conv_id: Some(cid),
            class_id,
        }))
    } else {
        Ok(None)
    }
}

fn can_moderate(a: &AuthUser, class_id: Option<i64>) -> bool {
    a.role == "root" || (a.role == "class_admin" && a.class_id.is_some() && a.class_id == class_id)
}

async fn list_notifications(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<NotificationDto>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,kind,text,created_at,read FROM notifications WHERE uid=?1 ORDER BY id DESC LIMIT 100",
    )?;
    let rows = stmt.query_map(params![a.uid], |r| {
        Ok(NotificationDto {
            id: r.get(0)?,
            kind: r.get(1)?,
            text: r.get(2)?,
            created_at: r.get(3)?,
            read: r.get::<_, i64>(4)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn mark_notifications_read(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    conn.execute("UPDATE notifications SET read=1 WHERE uid=?1 AND read=0", params![a.uid])?;
    Ok(Json(json!({"ok":true})))
}

async fn list_announcements(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<AnnouncementDto>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT a.id,a.scope,a.class_id,c.name,a.title,a.body,a.created_by,u.display_name,a.created_at
         FROM announcements a LEFT JOIN classes c ON c.id=a.class_id JOIN users u ON u.uid=a.created_by
         WHERE a.scope='global' OR a.class_id=?1 ORDER BY a.id DESC LIMIT 100",
    )?;
    let rows = stmt.query_map(params![a.class_id], |r| {
        Ok(AnnouncementDto {
            id: r.get(0)?,
            scope: r.get(1)?,
            class_id: r.get(2)?,
            class_name: r.get(3)?,
            title: r.get(4)?,
            body: r.get(5)?,
            created_by: r.get(6)?,
            created_by_name: r.get(7)?,
            created_at: r.get(8)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn create_announcement(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateAnnouncementRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let title = req.title.trim();
    let body = req.body.trim();
    if title.is_empty() || title.chars().count() > 100 {
        return Err(AppError::bad("公告标题需 1-100 字"));
    }
    if body.is_empty() || body.chars().count() > 5000 {
        return Err(AppError::bad("公告内容需 1-5000 字"));
    }
    let (scope, class_id): (String, Option<i64>) = match req.scope.as_str() {
        "global" => {
            if a.role != "root" {
                return Err(AppError::forbidden("只有总管理员可以发布全校公告"));
            }
            ("global".to_string(), None)
        }
        "class" => {
            let cid = if a.role == "class_admin" {
                a.class_id.ok_or_else(|| AppError::forbidden("你还没有管理班级"))?
            } else {
                req.class_id.ok_or_else(|| AppError::bad("缺少班级"))?
            };
            let exists: Option<i64> = conn_query_class(&state, cid)?;
            if exists.is_none() {
                return Err(AppError::not_found("班级不存在"));
            }
            ("class".to_string(), Some(cid))
        }
        _ => return Err(AppError::bad("未知的公告范围")),
    };
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO announcements (scope,class_id,title,body,created_by,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![scope, class_id, title, body, a.uid, now()],
    )?;
    let id = tx.last_insert_rowid();
    let text = format!("新公告：{title}");
    let uids: Vec<i64> = {
        let mut stmt = if class_id.is_some() {
            tx.prepare("SELECT uid FROM users WHERE status='active' AND class_id=?1")?
        } else {
            tx.prepare("SELECT uid FROM users WHERE status='active'")?
        };
        let rows = if let Some(cid) = class_id {
            stmt.query_map(params![cid], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map([], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        rows
    };
    for uid in uids {
        notify(&tx, uid, "announcement", &text)?;
    }
    audit(&tx, &state.hmac, Some(a.uid), "admin.announcement.create", Some("announcement"), Some(&id.to_string()), None)?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"id":id})))
}

fn conn_query_class(state: &AppState, cid: i64) -> ApiResult<Option<i64>> {
    let conn = state.db.lock().unwrap();
    Ok(conn
        .query_row("SELECT id FROM classes WHERE id=?1 AND archived=0", params![cid], |r| r.get(0))
        .optional()?)
}

async fn delete_announcement(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let (scope, class_id): (String, Option<i64>) = conn
        .query_row("SELECT scope,class_id FROM announcements WHERE id=?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("公告不存在"))?;
    if a.role != "root" && !(a.role == "class_admin" && scope == "class" && class_id == a.class_id) {
        return Err(AppError::forbidden("无权删除该公告"));
    }
    conn.execute("DELETE FROM announcements WHERE id=?1", params![id])?;
    audit(&conn, &state.hmac, Some(a.uid), "admin.announcement.delete", Some("announcement"), Some(&id.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn create_report(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateReportRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !rate_allow(&state, a.uid, "report", 10, 3600) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate", "举报过于频繁，请稍后再试"));
    }
    let reason = req.reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(AppError::bad("举报理由需 1-500 字"));
    }
    if req.target_type != "user" && req.target_type != "message" {
        return Err(AppError::bad("未知的举报类型"));
    }
    let conn = state.db.lock().unwrap();
    let target = report_target(&conn, &req.target_type, req.target_id)?
        .ok_or_else(|| AppError::not_found("举报对象不存在"))?;
    if req.target_type == "user" && target.uid == Some(a.uid) {
        return Err(AppError::bad("不能举报自己"));
    }
    if let Some(cid) = target.conv_id {
        let member: i64 = conn.query_row(
            "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
            params![cid, a.uid],
            |r| r.get(0),
        )?;
        if member == 0 {
            return Err(AppError::forbidden("不在该会话中"));
        }
    }
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM reports WHERE reporter_uid=?1 AND target_type=?2 AND target_id=?3 AND status='pending'",
        params![a.uid, req.target_type, req.target_id],
        |r| r.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::conflict("已举报，请等待处理"));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO reports (reporter_uid,target_type,target_id,reason,status,created_at) VALUES (?1,?2,?3,?4,'pending',?5)",
        params![a.uid, req.target_type, req.target_id, reason, now()],
    )?;
    let id = tx.last_insert_rowid();
    let admins: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT uid FROM users WHERE status='active' AND (role='root' OR (role='class_admin' AND class_id=?1))",
        )?;
        let v = stmt
            .query_map(params![target.class_id], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    for uid in admins {
        notify(&tx, uid, "report", "收到新的举报，请在管理后台处理")?;
    }
    audit(&tx, &state.hmac, Some(a.uid), "report.create", Some(&req.target_type), Some(&req.target_id.to_string()), None)?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"id":id})))
}

async fn admin_reports(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<ReportDto>>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let base = {
        let mut stmt = conn.prepare(
            "SELECT id,reporter_uid,target_type,target_id,reason,status,created_at,handled_by,action,detail
             FROM reports ORDER BY (status='pending') DESC, id DESC LIMIT 200",
        )?;
        let v = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let mut out = Vec::new();
    for (id, reporter_uid, target_type, target_id, reason, status, created_at, handled_by, action, detail) in base {
        let (reporter_name, reporter_class): (String, Option<i64>) = conn.query_row(
            "SELECT display_name,class_id FROM users WHERE uid=?1",
            params![reporter_uid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let target = report_target(&conn, &target_type, target_id)?;
        let target_class = target.as_ref().and_then(|t| t.class_id);
        if a.role != "root" && !(reporter_class == a.class_id || target_class == a.class_id) {
            continue;
        }
        out.push(ReportDto {
            id,
            reporter_uid,
            reporter_name,
            target_type,
            target_id,
            target_uid: target.as_ref().and_then(|t| t.uid),
            target_name: target.as_ref().and_then(|t| t.name.clone()),
            message_id: target.as_ref().and_then(|t| t.message_id),
            conv_id: target.as_ref().and_then(|t| t.conv_id),
            reason,
            status,
            created_at,
            handled_by,
            action,
            detail,
        });
    }
    Ok(Json(out))
}

async fn resolve_report(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(req): Json<ResolveReportRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let (reporter_uid, target_type, target_id, status): (i64, String, i64, String) = conn
        .query_row(
            "SELECT reporter_uid,target_type,target_id,status FROM reports WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("举报不存在"))?;
    if status != "pending" {
        return Err(AppError::conflict("该举报已处理"));
    }
    let target = report_target(&conn, &target_type, target_id)?.ok_or_else(|| AppError::not_found("举报对象不存在"))?;
    if !can_moderate(&a, target.class_id) {
        return Err(AppError::forbidden("无权处理该举报"));
    }
    let tx = conn.unchecked_transaction()?;
    let detail = match req.action.as_str() {
        "dismiss" => "已忽略".to_string(),
        "hide_message" => {
            let mid = target.message_id.ok_or_else(|| AppError::bad("举报对象不是消息"))?;
            tx.execute("UPDATE messages SET hidden=1 WHERE id=?1", params![mid])?;
            "已隐藏消息".to_string()
        }
        "mute" => {
            let uid = target.uid.ok_or_else(|| AppError::bad("找不到被举报用户"))?;
            let mins = req.minutes.unwrap_or(60).clamp(1, 10080);
            tx.execute("UPDATE users SET muted_until=?1 WHERE uid=?2", params![now() + mins * 60, uid])?;
            notify(&tx, uid, "moderation", &format!("你已被禁言 {mins} 分钟"))?;
            format!("已禁言 {mins} 分钟")
        }
        "ban" => {
            let uid = target.uid.ok_or_else(|| AppError::bad("找不到被举报用户"))?;
            if uid == a.uid {
                return Err(AppError::bad("不能封禁自己"));
            }
            tx.execute("UPDATE users SET status='banned' WHERE uid=?1", params![uid])?;
            tx.execute("UPDATE sessions SET revoked=1 WHERE uid=?1", params![uid])?;
            notify(&tx, uid, "moderation", "你的账号已被封禁，请联系管理员")?;
            "已封禁".to_string()
        }
        _ => return Err(AppError::bad("未知的处理方式")),
    };
    tx.execute(
        "UPDATE reports SET status='resolved',handled_by=?1,handled_at=?2,action=?3,detail=?4 WHERE id=?5",
        params![a.uid, now(), req.action, detail, id],
    )?;
    notify(&tx, reporter_uid, "report_result", &format!("你的举报已处理：{detail}"))?;
    audit(&tx, &state.hmac, Some(a.uid), "report.resolve", Some("report"), Some(&id.to_string()), Some(&detail))?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"ok":true})))
}

async fn admin_user_action(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>, Json(req): Json<UserActionRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    if uid == a.uid {
        return Err(AppError::bad("不能对自己执行该操作"));
    }
    let conn = state.db.lock().unwrap();
    let (role, class_id): (String, Option<i64>) = conn
        .query_row("SELECT role,class_id FROM users WHERE uid=?1", params![uid], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if a.role != "root" && (role != "student" || class_id != a.class_id) {
        return Err(AppError::forbidden("无权管理该用户"));
    }
    let tx = conn.unchecked_transaction()?;
    let detail = match req.action.as_str() {
        "mute" => {
            let mins = req.minutes.unwrap_or(60).clamp(1, 10080);
            tx.execute("UPDATE users SET muted_until=?1 WHERE uid=?2", params![now() + mins * 60, uid])?;
            notify(&tx, uid, "moderation", &format!("你已被禁言 {mins} 分钟"))?;
            format!("禁言 {mins} 分钟")
        }
        "unmute" => {
            tx.execute("UPDATE users SET muted_until=0 WHERE uid=?1", params![uid])?;
            "解除禁言".to_string()
        }
        "ban" => {
            tx.execute("UPDATE users SET status='banned' WHERE uid=?1", params![uid])?;
            tx.execute("UPDATE sessions SET revoked=1 WHERE uid=?1", params![uid])?;
            notify(&tx, uid, "moderation", "你的账号已被封禁，请联系管理员")?;
            "封禁".to_string()
        }
        "unban" => {
            tx.execute("UPDATE users SET status='active' WHERE uid=?1", params![uid])?;
            notify(&tx, uid, "moderation", "你的账号已解封")?;
            "解封".to_string()
        }
        _ => return Err(AppError::bad("未知的操作")),
    };
    audit(&tx, &state.hmac, Some(a.uid), "admin.user.action", Some("uid"), Some(&uid.to_string()), Some(&detail))?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"ok":true})))
}

fn load_transfer(conn: &Connection, id: i64) -> ApiResult<Option<TransferDto>> {
    Ok(conn
        .query_row(
            "SELECT t.id,t.uid,u.display_name,t.from_class_id,fc.name,t.to_class_id,tc.name,t.reason,t.status,t.out_by,t.in_by,t.created_at
             FROM transfers t JOIN users u ON u.uid=t.uid
             JOIN classes fc ON fc.id=t.from_class_id JOIN classes tc ON tc.id=t.to_class_id
             WHERE t.id=?1",
            params![id],
            |r| {
                Ok(TransferDto {
                    id: r.get(0)?,
                    uid: r.get(1)?,
                    display_name: r.get(2)?,
                    from_class_id: r.get(3)?,
                    from_class_name: r.get(4)?,
                    to_class_id: r.get(5)?,
                    to_class_name: r.get(6)?,
                    reason: r.get(7)?,
                    status: r.get(8)?,
                    out_by: r.get(9)?,
                    in_by: r.get(10)?,
                    created_at: r.get(11)?,
                })
            },
        )
        .optional()?)
}

async fn create_transfer(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateTransferRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let reason = req.reason.trim();
    if reason.chars().count() > 500 {
        return Err(AppError::bad("转班理由最多 500 字"));
    }
    let conn = state.db.lock().unwrap();
    let (role, class_id): (String, Option<i64>) = conn.query_row(
        "SELECT role,class_id FROM users WHERE uid=?1",
        params![a.uid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if role != "student" {
        return Err(AppError::forbidden("只有学生可以申请转班"));
    }
    let from = class_id.ok_or_else(|| AppError::bad("你还没有班级"))?;
    if req.to_class_id == from {
        return Err(AppError::bad("已经在这个班了"));
    }
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM classes WHERE id=?1 AND archived=0", params![req.to_class_id], |r| r.get(0))
        .optional()?;
    if exists.is_none() {
        return Err(AppError::not_found("目标班级不存在"));
    }
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM transfers WHERE uid=?1 AND status IN ('pending','out_approved','in_approved')",
        params![a.uid],
        |r| r.get(0),
    )?;
    if pending > 0 {
        return Err(AppError::conflict("已有待处理的转班申请"));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO transfers (uid,from_class_id,to_class_id,reason,status,created_at) VALUES (?1,?2,?3,?4,'pending',?5)",
        params![a.uid, from, req.to_class_id, reason, now()],
    )?;
    let id = tx.last_insert_rowid();
    let admins: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT uid FROM users WHERE status='active' AND (role='root' OR (role='class_admin' AND class_id IN (?1,?2)))",
        )?;
        let v = stmt
            .query_map(params![from, req.to_class_id], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    for uid in admins {
        notify(&tx, uid, "transfer", "收到新的转班申请，请在管理后台处理")?;
    }
    audit(&tx, &state.hmac, Some(a.uid), "transfer.create", Some("transfer"), Some(&id.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"id":id})))
}

async fn my_transfers(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<TransferDto>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT id FROM transfers WHERE uid=?1 ORDER BY id DESC LIMIT 20")?;
        let v = stmt
            .query_map(params![a.uid], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    let mut out = Vec::new();
    for id in ids {
        if let Some(t) = load_transfer(&conn, id)? {
            out.push(t);
        }
    }
    Ok(Json(out))
}

async fn admin_transfers(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<TransferDto>>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let ids: Vec<i64> = {
        let mut stmt = if a.role == "root" {
            conn.prepare("SELECT id FROM transfers ORDER BY (status IN ('pending','out_approved','in_approved')) DESC, id DESC LIMIT 200")?
        } else {
            conn.prepare(
                "SELECT id FROM transfers WHERE from_class_id=?1 OR to_class_id=?1 ORDER BY (status IN ('pending','out_approved','in_approved')) DESC, id DESC LIMIT 200",
            )?
        };
        let v = if a.role == "root" {
            stmt.query_map([], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            let cid = a.class_id.unwrap_or(0);
            stmt.query_map(params![cid], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        v
    };
    let mut out = Vec::new();
    for id in ids {
        if let Some(t) = load_transfer(&conn, id)? {
            out.push(t);
        }
    }
    Ok(Json(out))
}

async fn transfer_action(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(req): Json<TransferActionRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let t = load_transfer(&conn, id)?.ok_or_else(|| AppError::not_found("转班申请不存在"))?;
    if t.status == "approved" || t.status == "rejected" {
        return Err(AppError::conflict("该申请已结束"));
    }
    let authorized = a.role == "root" || a.class_id == Some(t.from_class_id) || a.class_id == Some(t.to_class_id);
    if !authorized {
        return Err(AppError::forbidden("无权处理该转班申请"));
    }
    let tx = conn.unchecked_transaction()?;
    if req.action == "reject" {
        tx.execute("UPDATE transfers SET status='rejected' WHERE id=?1", params![id])?;
        notify(
            &tx,
            t.uid,
            "transfer",
            &format!("转班申请被拒绝：{} → {}", t.from_class_name, t.to_class_name),
        )?;
        audit(&tx, &state.hmac, Some(a.uid), "transfer.reject", Some("transfer"), Some(&id.to_string()), None)?;
        tx.commit()?;
        let _ = state.ws.send(());
        return Ok(Json(json!({"ok":true})));
    }
    if req.action != "approve" {
        return Err(AppError::bad("未知的操作"));
    }
    let status: String;
    let mut out_by = t.out_by;
    let mut in_by = t.in_by;
    if a.role == "root" {
        status = "approved".to_string();
        out_by = Some(a.uid);
        in_by = Some(a.uid);
    } else if a.class_id == Some(t.from_class_id) {
        if t.status == "out_approved" {
            return Err(AppError::conflict("转出班级已同意"));
        }
        out_by = Some(a.uid);
        if t.status == "in_approved" {
            status = "approved".to_string();
        } else {
            status = "out_approved".to_string();
        }
    } else {
        if t.status == "in_approved" {
            return Err(AppError::conflict("转入班级已同意"));
        }
        in_by = Some(a.uid);
        if t.status == "out_approved" {
            status = "approved".to_string();
        } else {
            status = "in_approved".to_string();
        }
    }
    if status == "approved" {
        tx.execute(
            "UPDATE users SET class_id=?1, roster_id=NULL WHERE uid=?2",
            params![t.to_class_id, t.uid],
        )?;
        tx.execute("UPDATE roster_entries SET claimed_uid=NULL WHERE claimed_uid=?1", params![t.uid])?;
        let old_conv: Option<i64> = tx
            .query_row(
                "SELECT id FROM conversations WHERE kind='class_group' AND class_id=?1",
                params![t.from_class_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(c) = old_conv {
            tx.execute("DELETE FROM conversation_members WHERE conv_id=?1 AND uid=?2", params![c, t.uid])?;
        }
        rotate_group_key(&tx, t.from_class_id)?;
        rotate_group_key(&tx, t.to_class_id)?;
        notify(
            &tx,
            t.uid,
            "transfer",
            &format!("转班成功：{} → {}，请重新进入班级群", t.from_class_name, t.to_class_name),
        )?;
    } else {
        let side = if status == "out_approved" { "转出班级" } else { "转入班级" };
        notify(&tx, t.uid, "transfer", &format!("{side}已同意你的转班申请，等待另一方审批"))?;
    }
    tx.execute(
        "UPDATE transfers SET status=?1,
           out_by=COALESCE(?2,out_by), out_at=CASE WHEN ?2 IS NULL THEN out_at ELSE ?3 END,
           in_by=COALESCE(?4,in_by), in_at=CASE WHEN ?4 IS NULL THEN in_at ELSE ?3 END
         WHERE id=?5",
        params![status, out_by, now(), in_by, id],
    )?;
    audit(&tx, &state.hmac, Some(a.uid), "transfer.approve", Some("transfer"), Some(&id.to_string()), Some(&status))?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"ok":true,"status":status})))
}

async fn create_request(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<FriendRequestCreate>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if !rate_allow(&state, a.uid, "friend_request", 20, 3600) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate", "好友申请过于频繁，请稍后再试"));
    }
    if req.to_uid == a.uid {
        return Err(AppError::bad("不能添加自己"));
    }
    let conn = state.db.lock().unwrap();
    let target: Option<i64> = conn.query_row("SELECT uid FROM users WHERE uid=?1 AND status='active'", params![req.to_uid], |r| r.get(0)).optional()?;
    if target.is_none() {
        return Err(AppError::not_found("用户不存在"));
    }
    let blocked: i64 = conn.query_row(
        "SELECT COUNT(*) FROM blocks WHERE (uid=?1 AND blocked_uid=?2) OR (uid=?2 AND blocked_uid=?1)",
        params![a.uid, req.to_uid],
        |r| r.get(0),
    )?;
    if blocked > 0 {
        return Err(AppError::forbidden("无法向该用户发送好友申请"));
    }
    let friend: i64 = conn.query_row(
        "SELECT COUNT(*) FROM friends WHERE uid=?1 AND friend_uid=?2",
        params![a.uid, req.to_uid],
        |r| r.get(0),
    )?;
    if friend > 0 {
        return Err(AppError::conflict("已经是好友"));
    }
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM friend_requests WHERE from_uid=?1 AND to_uid=?2 AND status='pending'",
        params![a.uid, req.to_uid],
        |r| r.get(0),
    )?;
    if pending > 0 {
        return Err(AppError::conflict("申请已发送"));
    }
    if req.message_enc.len() > 4096 {
        return Err(AppError::bad("申请附言过长"));
    }
    conn.execute(
        "INSERT INTO friend_requests (from_uid,to_uid,message_enc,status,created_at) VALUES (?1,?2,?3,'pending',?4)",
        params![a.uid, req.to_uid, req.message_enc, now()],
    )?;
    let id = conn.last_insert_rowid();
    let from_name: String = conn.query_row("SELECT display_name FROM users WHERE uid=?1", params![a.uid], |r| r.get(0))?;
    notify(&conn, req.to_uid, "friend_request", &format!("{from_name} 请求加你为好友"))?;
    audit(&conn, &state.hmac, Some(a.uid), "friend.request", Some("uid"), Some(&req.to_uid.to_string()), None)?;
    let _ = state.ws.send(());
    Ok(Json(json!({"id":id})))
}

async fn list_requests(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<FriendRequestDto>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT r.id,r.from_uid,fu.display_name,fu.public_key,r.to_uid,tu.display_name,r.message_enc,r.status,r.created_at
         FROM friend_requests r JOIN users fu ON fu.uid=r.from_uid JOIN users tu ON tu.uid=r.to_uid
         WHERE r.from_uid=?1 OR r.to_uid=?1 ORDER BY r.id DESC LIMIT 100",
    )?;
    let rows = stmt.query_map(params![a.uid], |r| {
        Ok(FriendRequestDto {
            id: r.get(0)?,
            from_uid: r.get(1)?,
            from_name: r.get(2)?,
            from_public_key: r.get(3)?,
            to_uid: r.get(4)?,
            to_name: r.get(5)?,
            message_enc: r.get(6)?,
            status: r.get(7)?,
            created_at: r.get(8)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn accept_request(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>, Json(req): Json<AcceptFriendRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let (from_uid, to_uid, status): (i64, i64, String) = conn.query_row(
        "SELECT from_uid,to_uid,status FROM friend_requests WHERE id=?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).optional()?.ok_or_else(|| AppError::not_found("申请不存在"))?;
    if to_uid != a.uid || status != "pending" {
        return Err(AppError::forbidden("无权处理该申请"));
    }
    let pair = format!("direct:{}:{}", from_uid.min(to_uid), from_uid.max(to_uid));
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO conversations (kind,pair_key,current_key_version,created_at) VALUES ('direct',?1,?2,?3)",
        params![pair, req.key_version, now()],
    )?;
    let conv_id: i64 = tx.query_row("SELECT id FROM conversations WHERE pair_key=?1", params![pair], |r| r.get(0))?;
    tx.execute(
        "INSERT OR IGNORE INTO conversation_members (conv_id,uid,last_read,joined_at) VALUES (?1,?2,0,?3)",
        params![conv_id, from_uid, now()],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO conversation_members (conv_id,uid,last_read,joined_at) VALUES (?1,?2,0,?3)",
        params![conv_id, to_uid, now()],
    )?;
    let mut have = Vec::new();
    for k in &req.keys {
        tx.execute(
            "INSERT OR REPLACE INTO conversation_keys (conv_id,key_version,recipient_uid,wrapped_key,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![conv_id, req.key_version, k.recipient_uid, k.wrapped_key, now()],
        )?;
        have.push(k.recipient_uid);
    }
    if !have.contains(&from_uid) || !have.contains(&to_uid) {
        return Err(AppError::bad("会话密钥包裹不完整"));
    }
    tx.execute("INSERT OR IGNORE INTO friends (uid,friend_uid,created_at) VALUES (?1,?2,?3)", params![from_uid, to_uid, now()])?;
    tx.execute("INSERT OR IGNORE INTO friends (uid,friend_uid,created_at) VALUES (?1,?2,?3)", params![to_uid, from_uid, now()])?;
    tx.execute("UPDATE friend_requests SET status='accepted',handled_at=?1 WHERE id=?2", params![now(), id])?;
    let to_name: String = tx.query_row("SELECT display_name FROM users WHERE uid=?1", params![to_uid], |r| r.get(0))?;
    notify(&tx, from_uid, "friend_accept", &format!("{to_name} 已同意你的好友申请"))?;
    audit(&tx, &state.hmac, Some(a.uid), "friend.accept", Some("uid"), Some(&from_uid.to_string()), None)?;
    tx.commit()?;
    let _ = state.ws.send(());
    Ok(Json(json!({"conv_id":conv_id})))
}

async fn decline_request(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let to_uid: Option<i64> = conn.query_row("SELECT to_uid FROM friend_requests WHERE id=?1 AND status='pending'", params![id], |r| r.get(0)).optional()?;
    if to_uid != Some(a.uid) {
        return Err(AppError::forbidden("无权处理该申请"));
    }
    conn.execute("UPDATE friend_requests SET status='declined',handled_at=?1 WHERE id=?2", params![now(), id])?;
    Ok(Json(json!({"ok":true})))
}

async fn list_friends(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<FriendDto>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT u.uid,u.handle_display,u.display_name,u.public_key,u.avatar_file_id FROM friends f JOIN users u ON u.uid=f.friend_uid WHERE f.uid=?1 ORDER BY u.display_name",
    )?;
    let rows = stmt.query_map(params![a.uid], |r| {
        Ok(FriendDto { uid: r.get(0)?, handle: r.get(1)?, display_name: r.get(2)?, public_key: r.get(3)?, avatar_file_id: r.get(4)? })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn list_conversations(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<ConversationInfo>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT c.id,c.kind,c.class_id,c.current_key_version,cm.last_read FROM conversations c JOIN conversation_members cm ON cm.conv_id=c.id WHERE cm.uid=?1 ORDER BY c.id",
    )?;
    let rows = stmt.query_map(params![a.uid], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (conv_id, kind, class_id, key_version, last_read) = row?;
        let member_wrap: String = conn
            .query_row(
                "SELECT wrapped_key FROM conversation_keys WHERE conv_id=?1 AND key_version=?2 AND recipient_uid=?3",
                params![conv_id, key_version, a.uid],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default();
        let title: String = if kind == "class_group" {
            conn.query_row("SELECT name FROM classes WHERE id=?1", params![class_id.unwrap_or(0)], |r| r.get(0)).unwrap_or_else(|_| "班级群".into())
        } else {
            conn.query_row(
                "SELECT u.display_name FROM conversation_members cm JOIN users u ON u.uid=cm.uid WHERE cm.conv_id=?1 AND cm.uid!=?2 LIMIT 1",
                params![conv_id, a.uid],
                |r| r.get(0),
            ).unwrap_or_else(|_| "私聊".into())
        };
        let unread: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE conv_id=?1 AND id>?2 AND sender_uid!=?3",
            params![conv_id, last_read, a.uid],
            |r| r.get(0),
        )?;
        let last_at: i64 = conn.query_row(
            "SELECT COALESCE(MAX(created_at),0) FROM messages WHERE conv_id=?1",
            params![conv_id],
            |r| r.get(0),
        )?;
        out.push(ConversationInfo { id: conv_id, kind, title, class_id, key_version, member_wrap, unread, last_at, last_preview: String::new() });
    }
    out.sort_by(|x, y| y.last_at.cmp(&x.last_at));
    Ok(Json(out))
}

fn msg_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MessageDto> {
    Ok(MessageDto {
        id: r.get(0)?,
        conv_id: r.get(1)?,
        sender_uid: r.get(2)?,
        sender_name: r.get(3)?,
        key_version: r.get(4)?,
        client_msg_id: r.get(5)?,
        ciphertext: r.get(6)?,
        created_at: r.get(7)?,
    })
}

async fn list_messages(State(state): State<AppState>, headers: HeaderMap, Path(conv_id): Path<i64>, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Vec<MessageDto>>> {
    let a = auth(&state, &headers)?;
    let after: i64 = q.get("after").and_then(|s| s.parse().ok()).unwrap_or(0);
    let conn = state.db.lock().unwrap();
    let member: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
        params![conv_id, a.uid],
        |r| r.get(0),
    )?;
    if member == 0 {
        return Err(AppError::forbidden("不在该会话中"));
    }
    let out: Vec<MessageDto> = if after == 0 {
        let mut stmt = conn.prepare(
            "SELECT m.id,m.conv_id,m.sender_uid,u.display_name,m.key_version,m.client_msg_id,m.ciphertext,m.created_at FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE m.conv_id=?1 AND m.hidden=0 ORDER BY m.id DESC LIMIT 200",
        )?;
        let mut v = stmt.query_map(params![conv_id], msg_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        v.reverse();
        v
    } else {
        let mut stmt = conn.prepare(
            "SELECT m.id,m.conv_id,m.sender_uid,u.display_name,m.key_version,m.client_msg_id,m.ciphertext,m.created_at FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE m.conv_id=?1 AND m.id>?2 AND m.hidden=0 ORDER BY m.id ASC LIMIT 200",
        )?;
        let v = stmt.query_map(params![conv_id, after], msg_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        v
    };
    Ok(Json(out))
}

async fn send_message(State(state): State<AppState>, headers: HeaderMap, Path(conv_id): Path<i64>, Json(req): Json<SendMessageRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if req.ciphertext.len() > 65536 || req.client_msg_id.is_empty() || req.client_msg_id.len() > 64 {
        return Err(AppError::bad("消息格式不正确"));
    }
    if !rate_allow(&state, a.uid, "message", 30, 60) {
        return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate", "发送太快，请稍后再试"));
    }
    let conn = state.db.lock().unwrap();
    let member: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
        params![conv_id, a.uid],
        |r| r.get(0),
    )?;
    if member == 0 {
        return Err(AppError::forbidden("不在该会话中"));
    }
    let muted_until: i64 = conn.query_row("SELECT muted_until FROM users WHERE uid=?1", params![a.uid], |r| r.get(0))?;
    if muted_until > now() {
        return Err(AppError::forbidden("你已被禁言，暂时不能发送消息"));
    }
    let (kind, _current): (String, i64) = conn.query_row(
        "SELECT kind,current_key_version FROM conversations WHERE id=?1",
        params![conv_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )?;
    if kind == "direct" {
        let other: Option<i64> = conn
            .query_row(
                "SELECT uid FROM conversation_members WHERE conv_id=?1 AND uid<>?2 LIMIT 1",
                params![conv_id, a.uid],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(other) = other {
            let blocked: i64 = conn.query_row(
                "SELECT COUNT(*) FROM blocks WHERE (uid=?1 AND blocked_uid=?2) OR (uid=?2 AND blocked_uid=?1)",
                params![a.uid, other],
                |r| r.get(0),
            )?;
            if blocked > 0 {
                return Err(AppError::forbidden("消息无法送达"));
            }
        }
    }
    conn.execute(
        "INSERT OR IGNORE INTO messages (conv_id,sender_uid,key_version,client_msg_id,ciphertext,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![conv_id, a.uid, req.key_version, req.client_msg_id, req.ciphertext, now()],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM messages WHERE sender_uid=?1 AND client_msg_id=?2",
        params![a.uid, req.client_msg_id],
        |r| r.get(0),
    )?;
    let _ = state.ws.send(());
    Ok(Json(json!({"id":id})))
}

async fn mark_read(State(state): State<AppState>, headers: HeaderMap, Path(conv_id): Path<i64>, Json(req): Json<ReadRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    conn.execute(
        "UPDATE conversation_members SET last_read=MAX(last_read,?1) WHERE conv_id=?2 AND uid=?3",
        params![req.last_read, conv_id, a.uid],
    )?;
    Ok(Json(json!({"ok":true})))
}

async fn conversation_keys(State(state): State<AppState>, headers: HeaderMap, Path(conv_id): Path<i64>) -> ApiResult<Json<Vec<Value>>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let member: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
        params![conv_id, a.uid],
        |r| r.get(0),
    )?;
    if member == 0 {
        return Err(AppError::forbidden("不在该会话中"));
    }
    let mut stmt = conn.prepare(
        "SELECT key_version,wrapped_key FROM conversation_keys WHERE conv_id=?1 AND recipient_uid=?2 ORDER BY key_version",
    )?;
    let rows = stmt
        .query_map(params![conv_id, a.uid], |r| {
            Ok(json!({"key_version": r.get::<_, i64>(0)?, "wrapped_key": r.get::<_, String>(1)?}))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Json(rows))
}

async fn class_escrow(State(state): State<AppState>, headers: HeaderMap, Path(class_id): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let (pk, wrap): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT escrow_public_key,escrow_priv_for_root FROM classes WHERE id=?1",
            params![class_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("班级不存在"))?;
    Ok(Json(json!({"escrow_public_key": pk, "escrow_priv_for_root": wrap})))
}

async fn admin_stats(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<AdminStats>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" && a.role != "class_admin" {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let (users, classes, pending): (i64, i64, i64) = if a.role == "root" {
        (
            conn.query_row("SELECT COUNT(*) FROM users WHERE role!='root'", [], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM classes WHERE archived=0", [], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM friend_requests WHERE status='pending'", [], |r| r.get(0))?,
        )
    } else {
        let cid = a.class_id.unwrap_or(0);
        (
            conn.query_row("SELECT COUNT(*) FROM users WHERE class_id=?1", params![cid], |r| r.get(0))?,
            1,
            conn.query_row("SELECT COUNT(*) FROM friend_requests r JOIN users u ON u.uid=r.to_uid WHERE r.status='pending' AND u.class_id=?1", params![cid], |r| r.get(0))?,
        )
    };
    let online: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT s.uid) FROM sessions s WHERE s.revoked=0 AND s.expires_at>?1",
        params![now()],
        |r| r.get(0),
    )?;
    let (messages_today, messages_total): (i64, i64) = if a.role == "root" {
        (
            conn.query_row("SELECT COUNT(*) FROM messages WHERE created_at>?1", params![now() - 86400], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?,
        )
    } else {
        let cid = a.class_id.unwrap_or(0);
        (
            conn.query_row("SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1 AND m.created_at>?2", params![cid, now() - 86400], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1", params![cid], |r| r.get(0))?,
        )
    };
    let db_bytes = std::fs::metadata(state.data.join("liaisonyy.db")).map(|m| m.len() as i64).unwrap_or(0);
    let cid = a.class_id.unwrap_or(0);
    let (reports_pending, muted, banned): (i64, i64, i64) = if a.role == "root" {
        (
            conn.query_row("SELECT COUNT(*) FROM reports WHERE status='pending'", [], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE muted_until>?1", params![now()], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE status='banned'", [], |r| r.get(0))?,
        )
    } else {
        (
            conn.query_row("SELECT COUNT(*) FROM reports r JOIN users u ON u.uid=r.reporter_uid WHERE r.status='pending' AND u.class_id=?1", params![cid], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE muted_until>?1 AND class_id=?2", params![now(), cid], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE status='banned' AND class_id=?1", params![cid], |r| r.get(0))?,
        )
    };
    Ok(Json(AdminStats {
        users,
        online,
        messages_today,
        messages_total,
        classes,
        pending_requests: pending,
        db_bytes,
        reports_pending,
        muted,
        banned,
    }))
}

async fn admin_audit(State(state): State<AppState>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> ApiResult<Json<Vec<AuditEntry>>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let after: i64 = q.get("after").and_then(|s| s.parse().ok()).unwrap_or(0);
    let action = q.get("action").map(|s| s.trim().to_string()).unwrap_or_default();
    let limit: i64 = q.get("limit").and_then(|s| s.parse().ok()).unwrap_or(200).clamp(1, 1000);
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,ts,actor_uid,action,target_type,target_id,detail FROM audit_log
         WHERE id>?1 AND (?2='' OR action LIKE ?2 || '%') ORDER BY id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![after, action, limit], |r| {
        Ok(AuditEntry { id: r.get(0)?, ts: r.get(1)?, actor_uid: r.get(2)?, action: r.get(3)?, target_type: r.get(4)?, target_id: r.get(5)?, detail: r.get(6)? })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
}

async fn admin_audit_verify(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<AuditVerify>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,prev_hash,hash,ts,actor_uid,action,target_type,target_id,detail FROM audit_log ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut prev = String::new();
    let mut checked = 0i64;
    let mut first_bad = None;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let row_prev: String = r.get(1)?;
        let row_hash: String = r.get(2)?;
        let ts: i64 = r.get(3)?;
        let actor: Option<i64> = r.get(4)?;
        let action: String = r.get(5)?;
        let tt: Option<String> = r.get(6)?;
        let tid: Option<String> = r.get(7)?;
        let detail: Option<String> = r.get(8)?;
        let payload = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            prev,
            ts,
            actor.unwrap_or(0),
            action,
            tt.unwrap_or_default(),
            tid.unwrap_or_default(),
            detail.unwrap_or_default()
        );
        let expect = hmac_sha256_b64(&state.hmac, payload.as_bytes());
        if row_prev != prev || row_hash != expect {
            first_bad = Some(id);
            break;
        }
        prev = row_hash;
        checked += 1;
    }
    Ok(Json(AuditVerify { ok: first_bad.is_none(), checked, first_bad }))
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

async fn admin_audit_csv(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id,ts,actor_uid,action,target_type,target_id,detail,prev_hash,hash FROM audit_log ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = String::from("id,ts,actor_uid,action,target_type,target_id,detail,prev_hash,hash\n");
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let ts: i64 = r.get(1)?;
        let actor: Option<i64> = r.get(2)?;
        let action: String = r.get(3)?;
        let tt: Option<String> = r.get(4)?;
        let tid: Option<String> = r.get(5)?;
        let detail: Option<String> = r.get(6)?;
        let prev: String = r.get(7)?;
        let hash: String = r.get(8)?;
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            id,
            ts,
            actor.map(|v| v.to_string()).unwrap_or_default(),
            csv_field(&action),
            csv_field(tt.as_deref().unwrap_or("")),
            csv_field(tid.as_deref().unwrap_or("")),
            csv_field(detail.as_deref().unwrap_or("")),
            csv_field(&prev),
            csv_field(&hash)
        ));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"liaisonyy-audit.csv\"")
        .body(Body::from(out))
        .map_err(|e| AppError::internal(e.to_string()))
}

fn dir_size(path: &FsPath) -> i64 {
    let mut total = 0i64;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                if md.is_dir() {
                    total += dir_size(&e.path());
                } else {
                    total += md.len() as i64;
                }
            }
        }
    }
    total
}

fn copy_dir_all(src: &FsPath, dst: &FsPath) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

async fn admin_dashboard(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<AdminDashboard>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let cid = a.class_id.unwrap_or(0);
    let (users, messages_today, messages_total): (i64, i64, i64) = if a.role == "root" {
        (
            conn.query_row("SELECT COUNT(*) FROM users WHERE role!='root'", [], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM messages WHERE created_at>?1", params![now() - 86400], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?,
        )
    } else {
        (
            conn.query_row("SELECT COUNT(*) FROM users WHERE class_id=?1", params![cid], |r| r.get(0))?,
            conn.query_row(
                "SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1 AND m.created_at>?2",
                params![cid, now() - 86400],
                |r| r.get(0),
            )?,
            conn.query_row(
                "SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1",
                params![cid],
                |r| r.get(0),
            )?,
        )
    };
    let sessions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE revoked=0 AND expires_at>?1",
        params![now()],
        |r| r.get(0),
    )?;
    let online: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT uid) FROM sessions WHERE revoked=0 AND expires_at>?1",
        params![now()],
        |r| r.get(0),
    )?;
    let auth_fail_today: i64 = if a.role == "root" {
        conn.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action='auth.login.fail' AND ts>?1",
            params![now() - 86400],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM audit_log a JOIN users u ON u.uid=a.actor_uid WHERE a.action='auth.login.fail' AND a.ts>?1 AND u.class_id=?2",
            params![now() - 86400, cid],
            |r| r.get(0),
        )?
    };
    let class_rows: Vec<(i64, String)> = {
        let mut stmt = if a.role == "root" {
            conn.prepare("SELECT id,name FROM classes WHERE archived=0 ORDER BY id")?
        } else {
            conn.prepare("SELECT id,name FROM classes WHERE archived=0 AND id=?1")?
        };
        let v = if a.role == "root" {
            stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![cid], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        v
    };
    let mut classes = Vec::new();
    for (id, name) in class_rows {
        classes.push(ClassStat {
            class_id: id,
            name,
            users: conn.query_row("SELECT COUNT(*) FROM users WHERE class_id=?1 AND role!='root'", params![id], |r| r.get(0))?,
            messages_today: conn.query_row(
                "SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1 AND m.created_at>?2",
                params![id, now() - 86400],
                |r| r.get(0),
            )?,
            messages_total: conn.query_row(
                "SELECT COUNT(*) FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE u.class_id=?1",
                params![id],
                |r| r.get(0),
            )?,
        });
    }
    let (reports_pending, muted, banned): (i64, i64, i64) = if a.role == "root" {
        (
            conn.query_row("SELECT COUNT(*) FROM reports WHERE status='pending'", [], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE muted_until>?1", params![now()], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE status='banned'", [], |r| r.get(0))?,
        )
    } else {
        (
            conn.query_row(
                "SELECT COUNT(*) FROM reports r JOIN users u ON u.uid=r.reporter_uid WHERE r.status='pending' AND u.class_id=?1",
                params![cid],
                |r| r.get(0),
            )?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE muted_until>?1 AND class_id=?2", params![now(), cid], |r| r.get(0))?,
            conn.query_row("SELECT COUNT(*) FROM users WHERE status='banned' AND class_id=?1", params![cid], |r| r.get(0))?,
        )
    };
    let db_bytes = std::fs::metadata(state.data.join("liaisonyy.db")).map(|m| m.len() as i64).unwrap_or(0);
    Ok(Json(AdminDashboard {
        users,
        online,
        sessions,
        auth_fail_today,
        messages_today,
        messages_total,
        classes,
        db_bytes,
        media_bytes: dir_size(&state.data.join("media")),
        reports_pending,
        muted,
        banned,
    }))
}

async fn create_backup(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<BackupResult>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let name = format!("backup-{}", now());
    let dir = state.data.join("backups").join(&name);
    std::fs::create_dir_all(&dir)?;
    {
        let conn = state.db.lock().unwrap();
        conn.execute("VACUUM INTO ?1", params![dir.join("liaisonyy.db").to_string_lossy().to_string()])?;
        for f in ["keyring.bin", "media.key", "identity.txt"] {
            let src = state.data.join(f);
            if src.exists() {
                std::fs::copy(&src, dir.join(f))?;
            }
        }
        let media = state.data.join("media");
        if media.exists() {
            copy_dir_all(&media, &dir.join("media"))?;
        }
        audit(&conn, &state.hmac, Some(a.uid), "admin.backup.create", Some("backup"), Some(&name), None)?;
    }
    Ok(Json(BackupResult {
        name: name.clone(),
        path: dir.to_string_lossy().to_string(),
        bytes: dir_size(&dir),
    }))
}

async fn list_backups(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<BackupInfo>>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(state.data.join("backups")) {
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let created_at = name
                .strip_prefix("backup-")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            out.push(BackupInfo { name, created_at, bytes: dir_size(&e.path()) });
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Json(out))
}

async fn request_restore(State(state): State<AppState>, headers: HeaderMap, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    if name.is_empty() || name.len() > 64 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::bad("备份名称不正确"));
    }
    let dir = state.data.join("backups").join(&name);
    if !dir.join("liaisonyy.db").exists() {
        return Err(AppError::not_found("备份不存在"));
    }
    std::fs::write(state.data.join("restore_request.txt"), &name)?;
    let conn = state.db.lock().unwrap();
    audit(&conn, &state.hmac, Some(a.uid), "admin.backup.restore_request", Some("backup"), Some(&name), None)?;
    Ok(Json(json!({"ok":true,"message":"将在服务端下次启动时恢复"})))
}

async fn admin_user_conversations(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>) -> ApiResult<Json<Vec<ConversationInfo>>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let target_class: Option<i64> = conn
        .query_row("SELECT class_id FROM users WHERE uid=?1", params![uid], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if a.role == "class_admin" && target_class != a.class_id {
        return Err(AppError::forbidden("只能查看本班学生"));
    }
    let mut stmt = conn.prepare(
        "SELECT c.id,c.kind,c.class_id,c.current_key_version FROM conversations c
         JOIN conversation_members cm ON cm.conv_id=c.id WHERE cm.uid=?1 ORDER BY c.id",
    )?;
    let rows = stmt.query_map(params![uid], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (conv_id, kind, class_id, key_version) = row?;
        let title: String = if kind == "class_group" {
            conn.query_row("SELECT name FROM classes WHERE id=?1", params![class_id.unwrap_or(0)], |r| r.get(0))
                .unwrap_or_else(|_| "班级群".into())
        } else {
            conn.query_row(
                "SELECT u.display_name FROM conversation_members cm JOIN users u ON u.uid=cm.uid WHERE cm.conv_id=?1 AND cm.uid!=?2 LIMIT 1",
                params![conv_id, uid],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "私聊".into())
        };
        let last_at: i64 = conn.query_row(
            "SELECT COALESCE(MAX(created_at),0) FROM messages WHERE conv_id=?1",
            params![conv_id],
            |r| r.get(0),
        )?;
        out.push(ConversationInfo {
            id: conv_id,
            kind,
            title,
            class_id,
            key_version,
            member_wrap: String::new(),
            unread: 0,
            last_at,
            last_preview: String::new(),
        });
    }
    out.sort_by(|x, y| y.last_at.cmp(&x.last_at));
    Ok(Json(out))
}

async fn admin_conversation_messages(State(state): State<AppState>, headers: HeaderMap, Path(conv_id): Path<i64>) -> ApiResult<Json<Vec<MessageDto>>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    if a.role == "class_admin" {
        let cid = a.class_id.unwrap_or(0);
        let allowed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM conversation_members cm JOIN users u ON u.uid=cm.uid WHERE cm.conv_id=?1 AND u.class_id=?2",
            params![conv_id, cid],
            |r| r.get(0),
        )?;
        let group_class: Option<i64> = conn
            .query_row("SELECT class_id FROM conversations WHERE id=?1 AND kind='class_group'", params![conv_id], |r| r.get(0))
            .optional()?
            .flatten();
        if allowed == 0 && group_class != Some(cid) {
            return Err(AppError::forbidden("无权查看该会话"));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT m.id,m.conv_id,m.sender_uid,u.display_name,m.key_version,m.client_msg_id,m.ciphertext,m.created_at
         FROM messages m JOIN users u ON u.uid=m.sender_uid WHERE m.conv_id=?1 ORDER BY m.id DESC LIMIT 500",
    )?;
    let mut v = stmt.query_map(params![conv_id], msg_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    v.reverse();
    Ok(Json(v))
}

async fn admin_student_conv_keys(State(state): State<AppState>, headers: HeaderMap, Path((uid, conv_id)): Path<(i64, i64)>) -> ApiResult<Json<Vec<Value>>> {
    let a = auth(&state, &headers)?;
    if !is_admin(&a) {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let target_class: Option<i64> = conn
        .query_row("SELECT class_id FROM users WHERE uid=?1", params![uid], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if a.role == "class_admin" && target_class != a.class_id {
        return Err(AppError::forbidden("只能查看本班学生"));
    }
    let member: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
        params![conv_id, uid],
        |r| r.get(0),
    )?;
    if member == 0 {
        return Err(AppError::forbidden("该用户不在会话中"));
    }
    let mut stmt = conn.prepare(
        "SELECT key_version,wrapped_key FROM conversation_keys WHERE conv_id=?1 AND recipient_uid=?2 ORDER BY key_version",
    )?;
    let rows = stmt
        .query_map(params![conv_id, uid], |r| {
            Ok(json!({"key_version": r.get::<_, i64>(0)?, "wrapped_key": r.get::<_, String>(1)?}))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    audit(&conn, &state.hmac, Some(a.uid), "admin.decrypt.keys", Some("uid"), Some(&uid.to_string()), Some(&conv_id.to_string()))?;
    Ok(Json(rows))
}

async fn admin_classes(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<ClassInfo>>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    Ok(Json(load_classes(&conn)?))
}

async fn create_class(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateClassRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    if req.name.trim().is_empty() || req.escrow_public_key.is_empty() || req.escrow_priv_for_root.is_empty() {
        return Err(AppError::bad("参数不完整"));
    }
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO classes (name,grade,escrow_public_key,escrow_priv_for_root,created_at) VALUES (?1,?2,?3,?4,?5)",
        params![req.name, req.grade, req.escrow_public_key, req.escrow_priv_for_root, now()],
    )?;
    let class_id = tx.last_insert_rowid();
    ensure_class_group(&tx, class_id)?;
    rotate_group_key(&tx, class_id)?;
    audit(&tx, &state.hmac, Some(a.uid), "admin.class.create", Some("class"), Some(&class_id.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"id":class_id})))
}

async fn import_roster(State(state): State<AppState>, headers: HeaderMap, Path(class_id): Path<i64>, Json(req): Json<ImportRosterRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    if req.names.len() > 500 {
        return Err(AppError::bad("一次最多导入 500 个名字"));
    }
    let conn = state.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    let mut n = 0;
    for name in req.names {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 30 {
            continue;
        }
        tx.execute(
            "INSERT INTO roster_entries (class_id,real_name,status) VALUES (?1,?2,'normal')",
            params![class_id, name],
        )?;
        n += 1;
    }
    audit(&tx, &state.hmac, Some(a.uid), "admin.roster.import", Some("class"), Some(&class_id.to_string()), Some(&format!("{n} names")))?;
    tx.commit()?;
    Ok(Json(json!({"imported":n})))
}

async fn assign_class_admin(State(state): State<AppState>, headers: HeaderMap, Path(class_id): Path<i64>, Json(req): Json<AssignClassAdminRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let user_class: Option<i64> = conn.query_row("SELECT class_id FROM users WHERE uid=?1", params![req.uid], |r| r.get(0)).optional()?;
    if user_class != Some(class_id) {
        return Err(AppError::bad("该用户不在这个班级"));
    }
    conn.execute("UPDATE users SET role='class_admin' WHERE uid=?1", params![req.uid])?;
    conn.execute("UPDATE classes SET escrow_priv_for_admin=?1 WHERE id=?2", params![req.class_priv_for_admin, class_id])?;
    audit(&conn, &state.hmac, Some(a.uid), "admin.class.assign_admin", Some("uid"), Some(&req.uid.to_string()), None)?;
    Ok(Json(json!({"ok":true})))
}

async fn admin_student_keys(State(state): State<AppState>, headers: HeaderMap, Path(uid): Path<i64>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" && a.role != "class_admin" {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let (target_class, public_key): (Option<i64>, String) = conn.query_row(
        "SELECT class_id,public_key FROM users WHERE uid=?1",
        params![uid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).optional()?.ok_or_else(|| AppError::not_found("用户不存在"))?;
    if a.role == "class_admin" && target_class != a.class_id {
        return Err(AppError::forbidden("只能操作本班学生"));
    }
    let (user_priv, profile_wrap, class_wrap): (String, String, String) = if a.role == "root" {
        let up: String = conn.query_row("SELECT user_priv_adm FROM user_keys WHERE uid=?1", params![uid], |r| r.get(0))?;
        let pw: String = conn.query_row("SELECT profile_adm FROM profiles_enc WHERE uid=?1", params![uid], |r| r.get(0))?;
        let cw: String = conn.query_row("SELECT COALESCE(escrow_priv_for_root,'') FROM classes WHERE id=?1", params![target_class.unwrap_or(0)], |r| r.get(0))?;
        (up, pw, cw)
    } else {
        let up: String = conn.query_row("SELECT user_priv_class FROM user_keys WHERE uid=?1", params![uid], |r| r.get(0))?;
        let pw: String = conn.query_row("SELECT profile_class FROM profiles_enc WHERE uid=?1", params![uid], |r| r.get(0))?;
        let cw: String = conn.query_row("SELECT COALESCE(escrow_priv_for_admin,'') FROM classes WHERE id=?1", params![target_class.unwrap_or(0)], |r| r.get(0))?;
        (up, pw, cw)
    };
    audit(&conn, &state.hmac, Some(a.uid), "admin.student_keys", Some("uid"), Some(&uid.to_string()), None)?;
    Ok(Json(json!({
        "mode": if a.role == "root" { "root" } else { "class" },
        "user_priv": user_priv,
        "profile_wrap": profile_wrap,
        "class_wrap": class_wrap,
        "public_key": public_key,
        "class_id": target_class,
    })))
}

async fn create_reset_key(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<CreateResetKeyRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" && a.role != "class_admin" {
        return Err(AppError::forbidden("需要管理员权限"));
    }
    if req.code.len() != 8 || !req.code.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::bad("重置密钥需 8 位数字"));
    }
    let conn = state.db.lock().unwrap();
    let target_class: Option<i64> = conn.query_row("SELECT class_id FROM users WHERE uid=?1", params![req.uid], |r| r.get(0)).optional()?.ok_or_else(|| AppError::not_found("用户不存在"))?;
    if a.role == "class_admin" && target_class != a.class_id {
        return Err(AppError::forbidden("只能操作本班学生"));
    }
    let code_verify = hmac_sha256_b64(&state.hmac, req.code.as_bytes());
    conn.execute(
        "INSERT INTO reset_keys (uid,code_verify,expires_at,used,attempts,created_by,code_wrap_root,reset_salt,reset_wrap_priv,reset_wrap_profile,created_at) VALUES (?1,?2,?3,0,0,?4,?5,?6,?7,?8,?9)",
        params![req.uid, code_verify, req.expires_at, a.uid, req.code_wrap_root, req.reset_salt, req.reset_wrap_priv, req.reset_wrap_profile, now()],
    )?;
    let id = conn.last_insert_rowid();
    audit(&conn, &state.hmac, Some(a.uid), "admin.reset_key.create", Some("uid"), Some(&req.uid.to_string()), None)?;
    Ok(Json(json!({"id":id})))
}

async fn reset_fetch(State(state): State<AppState>, Json(req): Json<ResetFetchRequest>) -> ApiResult<Json<ResetFetchResponse>> {
    let conn = state.db.lock().unwrap();
    let key = req.uid_or_handle.trim();
    let uid: Option<i64> = if key.chars().all(|c| c.is_ascii_digit()) {
        conn.query_row("SELECT uid FROM users WHERE uid=?1", params![key.parse::<i64>().unwrap_or(-1)], |r| r.get(0)).optional()?
    } else {
        conn.query_row("SELECT uid FROM users WHERE handle_lower=?1", params![key.to_lowercase()], |r| r.get(0)).optional()?
    };
    let uid = uid.ok_or_else(|| AppError::not_found("账号不存在"))?;
    let row: Option<(i64, String, i64, i64, String, String, String)> = conn.query_row(
        "SELECT id,code_verify,expires_at,used,reset_salt,reset_wrap_priv,reset_wrap_profile FROM reset_keys WHERE uid=?1 ORDER BY id DESC LIMIT 1",
        params![uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    ).optional()?;
    let (id, verify, expires, used, salt, wp, wpr) = row.ok_or_else(|| AppError::not_found("没有可用的重置密钥"))?;
    if used != 0 || expires < now() {
        return Err(AppError::bad("重置密钥已过期或已使用"));
    }
    let code_verify = hmac_sha256_b64(&state.hmac, req.code.as_bytes());
    if code_verify != verify {
        conn.execute("UPDATE reset_keys SET attempts=attempts+1 WHERE id=?1", params![id])?;
        return Err(AppError::unauthorized("重置密钥不正确"));
    }
    Ok(Json(ResetFetchResponse { reset_salt: salt, reset_wrap_priv: wp, reset_wrap_profile: wpr }))
}

async fn reset(State(state): State<AppState>, Json(req): Json<ResetRequest>) -> ApiResult<Json<Value>> {
    validate_password(&req.new_password)?;
    if let Some(pin) = req.new_pin.as_deref() {
        validate_pin(pin)?;
    }
    let (_, verify) = derive_keys(&req.new_password, &req.pwd_salt, &req.pwd_params)?;
    let conn = state.db.lock().unwrap();
    let key = req.uid_or_handle.trim();
    let uid: Option<i64> = if key.chars().all(|c| c.is_ascii_digit()) {
        conn.query_row("SELECT uid FROM users WHERE uid=?1", params![key.parse::<i64>().unwrap_or(-1)], |r| r.get(0)).optional()?
    } else {
        conn.query_row("SELECT uid FROM users WHERE handle_lower=?1", params![key.to_lowercase()], |r| r.get(0)).optional()?
    };
    let uid = uid.ok_or_else(|| AppError::not_found("账号不存在"))?;
    let row: Option<(i64, String, i64, i64)> = conn.query_row(
        "SELECT id,code_verify,expires_at,used FROM reset_keys WHERE uid=?1 ORDER BY id DESC LIMIT 1",
        params![uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).optional()?;
    let (id, code_verify, expires, used) = row.ok_or_else(|| AppError::not_found("没有可用的重置密钥"))?;
    if used != 0 || expires < now() {
        return Err(AppError::bad("重置密钥已过期或已使用"));
    }
    if hmac_sha256_b64(&state.hmac, req.code.as_bytes()) != code_verify {
        conn.execute("UPDATE reset_keys SET attempts=attempts+1 WHERE id=?1", params![id])?;
        return Err(AppError::unauthorized("重置密钥不正确"));
    }
    let tx = conn.unchecked_transaction()?;
    let pwd_len = req.new_password.chars().count();
    if let Some(pin) = req.new_pin.as_deref() {
        let salt = req.pin_salt.as_deref().ok_or_else(|| AppError::bad("缺少 PIN salt"))?;
        let params_pin = req.pin_params.as_ref().ok_or_else(|| AppError::bad("缺少 PIN 参数"))?;
        let (_, pv) = derive_keys(pin, salt, params_pin)?;
        tx.execute(
            "UPDATE credentials SET pwd_salt=?1,pwd_m=?2,pwd_t=?3,pwd_p=?4,pwd_verify=?5,pwd_len=?6,pin_salt=?7,pin_m=?8,pin_t=?9,pin_p=?10,pin_verify=?11,pin_len=?12,failed_count=0,locked_until=0 WHERE uid=?13",
            params![req.pwd_salt, req.pwd_params.m, req.pwd_params.t, req.pwd_params.p, b64e(&verify), pwd_len, salt, params_pin.m, params_pin.t, params_pin.p, b64e(&pv), pin.chars().count(), uid],
        )?;
        tx.execute("UPDATE users SET login_policy='pin_allowed' WHERE uid=?1", params![uid])?;
        tx.execute("UPDATE user_keys SET own_priv=?1,pin_own=?2 WHERE uid=?3", params![req.own_priv, req.pin_own, uid])?;
        tx.execute("UPDATE profiles_enc SET profile_own=?1,profile_pin=?2 WHERE uid=?3", params![req.profile_own, req.profile_pin, uid])?;
    } else {
        tx.execute(
            "UPDATE credentials SET pwd_salt=?1,pwd_m=?2,pwd_t=?3,pwd_p=?4,pwd_verify=?5,pwd_len=?6,pin_salt=NULL,pin_m=NULL,pin_t=NULL,pin_p=NULL,pin_verify=NULL,pin_len=NULL,failed_count=0,locked_until=0 WHERE uid=?7",
            params![req.pwd_salt, req.pwd_params.m, req.pwd_params.t, req.pwd_params.p, b64e(&verify), pwd_len, uid],
        )?;
        tx.execute("UPDATE users SET login_policy='password_only' WHERE uid=?1", params![uid])?;
        tx.execute("UPDATE user_keys SET own_priv=?1,pin_own=NULL WHERE uid=?2", params![req.own_priv, uid])?;
        tx.execute("UPDATE profiles_enc SET profile_own=?1,profile_pin=NULL WHERE uid=?2", params![req.profile_own, uid])?;
    }
    tx.execute("UPDATE reset_keys SET used=1 WHERE id=?1", params![id])?;
    tx.execute("UPDATE sessions SET revoked=1 WHERE uid=?1", params![uid])?;
    audit(&tx, &state.hmac, Some(uid), "auth.reset", Some("uid"), Some(&uid.to_string()), None)?;
    tx.commit()?;
    Ok(Json(json!({"ok":true})))
}

async fn update_manifest(State(state): State<AppState>) -> Json<Value> {
    if let Ok(bytes) = std::fs::read(state.data.join("update").join("manifest.json")) {
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            return Json(v);
        }
    }
    Json(json!({
        "schema": 1,
        "version": state.version.as_str(),
        "min_client": state.version.as_str(),
        "files": {}
    }))
}

async fn update_package(State(state): State<AppState>) -> ApiResult<Response> {
    let bytes = std::fs::read(state.data.join("update").join("package.exe"))
        .map_err(|_| AppError::not_found("更新包不存在"))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal(e.to_string()))
}

fn update_pubkey() -> Option<[u8; 32]> {
    let s = std::env::var("LIAISONYY_UPDATE_PUBKEY").ok()?;
    let raw = b64d(s.trim()).ok()?;
    if raw.len() != 32 {
        return None;
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&raw);
    Some(k)
}

async fn admin_upload_update(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<UpdatePackageRequest>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let ver_ok = |s: &str| {
        !s.is_empty()
            && s.len() <= 32
            && s.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    if !ver_ok(&req.version) || !ver_ok(&req.min_client) {
        return Err(AppError::bad("版本号格式不正确"));
    }
    let bytes = b64d(&req.data).map_err(|_| AppError::bad("更新包数据无效"))?;
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return Err(AppError::bad("更新包大小需在 1B-20MB 之间"));
    }
    let sha = sha256_b64(&bytes);
    let pk = update_pubkey().ok_or_else(|| AppError::bad("服务端未配置 LIAISONYY_UPDATE_PUBKEY"))?;
    let vk = ed25519_dalek::VerifyingKey::from_bytes(&pk).map_err(|_| AppError::internal("更新公钥无效"))?;
    let sig_raw = b64d(&req.signature).map_err(|_| AppError::bad("签名格式不正确"))?;
    let sig = ed25519_dalek::Signature::from_slice(&sig_raw).map_err(|_| AppError::bad("签名格式不正确"))?;
    let msg = format!("liaisonyy-update-v1|{}|{}|{}", req.version, req.min_client, sha);
    vk.verify_strict(msg.as_bytes(), &sig)
        .map_err(|_| AppError::bad("更新包签名校验失败"))?;
    let dir = state.data.join("update");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("package.exe"), &bytes)?;
    let package_url = req
        .url
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .filter(|u| u.len() <= 500)
        .unwrap_or_else(|| "/api/v1/update/package".to_string());
    let manifest = json!({
        "schema": 1,
        "version": req.version,
        "min_client": req.min_client,
        "signature": req.signature,
        "files": {
            "windows": {
                "url": package_url,
                "sha256": sha,
                "size": bytes.len()
            }
        }
    });
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
    let conn = state.db.lock().unwrap();
    audit(&conn, &state.hmac, Some(a.uid), "admin.update.upload", Some("version"), Some(&req.version), Some(&sha))?;
    Ok(Json(manifest))
}

async fn admin_page() -> Response {
    let mut r = Html(ADMIN_HTML).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        header::HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    );
    h.insert(header::X_FRAME_OPTIONS, header::HeaderValue::from_static("DENY"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, header::HeaderValue::from_static("no-referrer"));
    r
}

async fn ws_handler(State(state): State<AppState>, Query(q): Query<HashMap<String, String>>, ws: WebSocketUpgrade) -> Response {
    let token = q.get("token").cloned().unwrap_or_default();
    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let h = hmac_sha256_b64(&state.hmac, token.as_bytes());
    let uid: Option<i64> = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT s.uid FROM sessions s JOIN users u ON u.uid=s.uid WHERE s.token_hmac=?1 AND s.revoked=0 AND s.expires_at>?2 AND u.status='active'",
            params![h, now()],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None)
    };
    if uid.is_none() {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let mut rx = state.ws.subscribe();
    ws.on_upgrade(move |mut socket| async move {
        loop {
            tokio::select! {
                r = rx.recv() => {
                    if r.is_err() {
                        break;
                    }
                    if socket.send(Message::Text("nudge".into())).await.is_err() {
                        break;
                    }
                }
                msg = socket.recv() => {
                    match msg {
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(_)) => break,
                        _ => {}
                    }
                }
            }
        }
    })
}

// ---------------- main ----------------

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS users (
  uid INTEGER PRIMARY KEY AUTOINCREMENT,
  handle_lower TEXT UNIQUE NOT NULL,
  handle_display TEXT NOT NULL,
  display_name TEXT NOT NULL,
  real_name TEXT NOT NULL,
  class_id INTEGER,
  roster_id INTEGER UNIQUE,
  role TEXT NOT NULL DEFAULT 'student',
  status TEXT NOT NULL DEFAULT 'active',
  login_policy TEXT NOT NULL DEFAULT 'pin_allowed',
  public_key TEXT NOT NULL,
  avatar_file_id INTEGER,
  muted_until INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS credentials (
  uid INTEGER PRIMARY KEY,
  pwd_salt TEXT NOT NULL, pwd_m INTEGER NOT NULL, pwd_t INTEGER NOT NULL, pwd_p INTEGER NOT NULL,
  pwd_verify TEXT NOT NULL, pwd_len INTEGER NOT NULL,
  pin_salt TEXT, pin_m INTEGER, pin_t INTEGER, pin_p INTEGER, pin_verify TEXT, pin_len INTEGER,
  failed_count INTEGER NOT NULL DEFAULT 0, locked_until INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS user_keys (
  uid INTEGER PRIMARY KEY,
  own_priv TEXT NOT NULL, pin_own TEXT,
  user_priv_adm TEXT NOT NULL, user_priv_class TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS profiles_enc (
  uid INTEGER PRIMARY KEY, ciphertext TEXT NOT NULL DEFAULT '',
  profile_own TEXT NOT NULL, profile_pin TEXT,
  profile_adm TEXT NOT NULL, profile_class TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS root_state (
  id INTEGER PRIMARY KEY CHECK(id=1),
  public_key TEXT NOT NULL, root_own TEXT NOT NULL, root_recovery TEXT NOT NULL,
  root_verify TEXT NOT NULL, root_salt TEXT NOT NULL,
  root_m INTEGER NOT NULL, root_t INTEGER NOT NULL, root_p INTEGER NOT NULL, root_len INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS classes (
  id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, grade TEXT NOT NULL DEFAULT '',
  escrow_public_key TEXT, escrow_priv_for_root TEXT, escrow_priv_for_admin TEXT,
  archived INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS roster_entries (
  id INTEGER PRIMARY KEY AUTOINCREMENT, class_id INTEGER NOT NULL, real_name TEXT NOT NULL,
  student_no TEXT, claimed_uid INTEGER, status TEXT NOT NULL DEFAULT 'normal'
);
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY, uid INTEGER NOT NULL, token_hmac TEXT NOT NULL,
  created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS friends (
  uid INTEGER NOT NULL, friend_uid INTEGER NOT NULL, created_at INTEGER NOT NULL,
  PRIMARY KEY(uid, friend_uid)
);
CREATE TABLE IF NOT EXISTS blocks (
  uid INTEGER NOT NULL, blocked_uid INTEGER NOT NULL, created_at INTEGER NOT NULL,
  PRIMARY KEY(uid, blocked_uid)
);
CREATE TABLE IF NOT EXISTS public_profiles (
  uid INTEGER PRIMARY KEY,
  avatar_file_id INTEGER,
  background_file_id INTEGER,
  signature TEXT NOT NULL DEFAULT '',
  bio TEXT NOT NULL DEFAULT '',
  gender TEXT,
  birthday TEXT,
  socials_json TEXT
);
CREATE TABLE IF NOT EXISTS files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  owner_uid INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  size INTEGER NOT NULL,
  mime TEXT NOT NULL,
  path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS friend_requests (
  id INTEGER PRIMARY KEY AUTOINCREMENT, from_uid INTEGER NOT NULL, to_uid INTEGER NOT NULL,
  message_enc TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'pending',
  created_at INTEGER NOT NULL, handled_at INTEGER
);
CREATE TABLE IF NOT EXISTS conversations (
  id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, class_id INTEGER,
  pair_key TEXT UNIQUE, current_key_version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS conversation_members (
  conv_id INTEGER NOT NULL, uid INTEGER NOT NULL, last_read INTEGER NOT NULL DEFAULT 0,
  joined_at INTEGER NOT NULL, PRIMARY KEY(conv_id, uid)
);
CREATE TABLE IF NOT EXISTS conversation_keys (
  conv_id INTEGER NOT NULL, key_version INTEGER NOT NULL, recipient_uid INTEGER NOT NULL,
  wrapped_key TEXT NOT NULL, created_at INTEGER NOT NULL,
  PRIMARY KEY(conv_id, key_version, recipient_uid)
);
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT, conv_id INTEGER NOT NULL, sender_uid INTEGER NOT NULL,
  key_version INTEGER NOT NULL, client_msg_id TEXT NOT NULL, ciphertext TEXT NOT NULL,
  created_at INTEGER NOT NULL, hidden INTEGER NOT NULL DEFAULT 0,
  UNIQUE(sender_uid, client_msg_id)
);
CREATE TABLE IF NOT EXISTS reset_keys (
  id INTEGER PRIMARY KEY AUTOINCREMENT, uid INTEGER NOT NULL, code_verify TEXT NOT NULL,
  expires_at INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0, attempts INTEGER NOT NULL DEFAULT 0,
  created_by INTEGER NOT NULL, code_wrap_root TEXT NOT NULL, reset_salt TEXT NOT NULL DEFAULT '',
  reset_wrap_priv TEXT NOT NULL DEFAULT '', reset_wrap_profile TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, actor_uid INTEGER,
  action TEXT NOT NULL, target_type TEXT, target_id TEXT, ip TEXT, detail TEXT,
  prev_hash TEXT NOT NULL, hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS notifications (
  id INTEGER PRIMARY KEY AUTOINCREMENT, uid INTEGER NOT NULL, kind TEXT NOT NULL,
  text TEXT NOT NULL, created_at INTEGER NOT NULL, read INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS announcements (
  id INTEGER PRIMARY KEY AUTOINCREMENT, scope TEXT NOT NULL, class_id INTEGER,
  title TEXT NOT NULL, body TEXT NOT NULL, created_by INTEGER NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS reports (
  id INTEGER PRIMARY KEY AUTOINCREMENT, reporter_uid INTEGER NOT NULL,
  target_type TEXT NOT NULL, target_id INTEGER NOT NULL, reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending', created_at INTEGER NOT NULL,
  handled_by INTEGER, handled_at INTEGER, action TEXT, detail TEXT
);
CREATE TABLE IF NOT EXISTS transfers (
  id INTEGER PRIMARY KEY AUTOINCREMENT, uid INTEGER NOT NULL,
  from_class_id INTEGER NOT NULL, to_class_id INTEGER NOT NULL,
  reason TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'pending',
  out_by INTEGER, out_at INTEGER, in_by INTEGER, in_at INTEGER, created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conv_id,id);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts);
CREATE INDEX IF NOT EXISTS idx_notifications_uid ON notifications(uid,id);
CREATE INDEX IF NOT EXISTS idx_reports_status ON reports(status,id);
CREATE INDEX IF NOT EXISTS idx_transfers_status ON transfers(status,id);
"#;

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    conn.execute_batch(SCHEMA)?;
    let cols = {
        let mut stmt = conn.prepare("PRAGMA table_info(users)")?;
        let v = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        v
    };
    if !cols.iter().any(|c| c == "avatar_file_id") {
        conn.execute("ALTER TABLE users ADD COLUMN avatar_file_id INTEGER", [])?;
    }
    if !cols.iter().any(|c| c == "muted_until") {
        conn.execute("ALTER TABLE users ADD COLUMN muted_until INTEGER NOT NULL DEFAULT 0", [])?;
    }
    Ok(())
}

fn apply_pending_restore(data: &FsPath) -> anyhow::Result<()> {
    let marker = data.join("restore_request.txt");
    if !marker.exists() {
        return Ok(());
    }
    let name = std::fs::read_to_string(&marker)?.trim().to_string();
    let valid = !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let src = data.join("backups").join(&name);
    if !valid || !src.join("liaisonyy.db").exists() {
        tracing::warn!("invalid restore request, ignored");
        let _ = std::fs::remove_file(&marker);
        return Ok(());
    }
    let safety = data.join(format!("pre-restore-{}", now()));
    std::fs::create_dir_all(&safety)?;
    for f in ["liaisonyy.db", "liaisonyy.db-wal", "liaisonyy.db-shm"] {
        let p = data.join(f);
        if p.exists() {
            std::fs::rename(&p, safety.join(f))?;
        }
    }
    for f in ["keyring.bin", "media.key"] {
        let s = src.join(f);
        if s.exists() {
            std::fs::copy(&s, data.join(f))?;
        }
    }
    let media = data.join("media");
    if media.exists() {
        std::fs::rename(&media, safety.join("media"))?;
    }
    if src.join("media").exists() {
        copy_dir_all(&src.join("media"), &media)?;
    }
    std::fs::copy(src.join("liaisonyy.db"), data.join("liaisonyy.db"))?;
    std::fs::remove_file(&marker)?;
    tracing::info!("restored backup {name}; previous data saved in {}", safety.display());
    Ok(())
}

fn load_or_create_media_key(data: &FsPath) -> anyhow::Result<[u8; 32]> {    use rand::RngCore;
    let p = data.join("media.key");
    if p.exists() {
        let bytes = std::fs::read(&p)?;
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    let mut key = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    std::fs::write(&p, key)?;
    Ok(key)
}

fn load_or_create_keyring(data: &FsPath) -> anyhow::Result<Vec<u8>> {
    use rand::RngCore;
    let p = data.join("keyring.bin");
    if p.exists() {
        Ok(std::fs::read(p)?)
    } else {
        let mut k = vec![0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut k);
        std::fs::write(&p, &k)?;
        Ok(k)
    }
}

fn load_or_create_setup_token(data: &FsPath, has_root: bool) -> anyhow::Result<String> {
    if has_root {
        return Ok(String::new());
    }
    let p = data.join("setup_token.txt");
    if p.exists() {
        Ok(std::fs::read_to_string(p)?.trim().to_string())
    } else {
        let t = random_code(8);
        std::fs::write(&p, &t)?;
        Ok(t)
    }
}

fn ensure_tls(data: &FsPath) -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = data.join("tls");
    std::fs::create_dir_all(&dir)?;
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    if !cert.exists() || !key.exists() {
        let ck = rcgen::generate_simple_self_signed(vec!["liaisonyy.local".to_string(), "localhost".to_string()])?;
        std::fs::write(&cert, ck.cert.pem())?;
        std::fs::write(&key, ck.key_pair.serialize_pem())?;
    }
    Ok((cert, key))
}

fn ntp_query(host: &str) -> Option<i64> {
    let addr = (host, 123u16).to_socket_addrs().ok()?.next()?;
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    let mut packet = [0u8; 48];
    packet[0] = 0x1b; // LI=0, VN=3, mode=3 (client)
    sock.send_to(&packet, addr).ok()?;
    let mut buf = [0u8; 48];
    let (n, _) = sock.recv_from(&mut buf).ok()?;
    if n < 48 {
        return None;
    }
    let secs = u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]) as i64;
    let unix = secs - 2_208_988_800;
    if unix > 1_600_000_000 {
        Some(unix)
    } else {
        None
    }
}

fn http_date_query(host: &str) -> Option<i64> {
    let addr = (host, 80u16).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    let req = format!(
        "HEAD / HTTP/1.0\r\nHost: {host}\r\nUser-Agent: liaisonyy-time-sync\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut resp = String::new();
    stream.read_to_string(&mut resp).ok()?;
    for line in resp.lines() {
        if line.len() > 5 && line[..5].eq_ignore_ascii_case("date:") {
            let value = line[5..].trim();
            if let Ok(t) = httpdate::parse_http_date(value) {
                return t
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as i64);
            }
        }
    }
    None
}

/// Fetch authoritative time from the internet (NTP, then HTTP Date) and anchor
/// the core clock to it. Returns the unix time on success.
fn sync_network_time() -> Option<i64> {
    for host in ["time.cloudflare.com", "ntp.aliyun.com", "pool.ntp.org"] {
        if let Some(t) = ntp_query(host) {
            liaisonyy_core::clock::init_network_time(t);
            println!("network time synced via NTP {host}: {t}");
            return Some(t);
        }
    }
    for host in ["www.baidu.com", "www.cloudflare.com", "www.github.com"] {
        if let Some(t) = http_date_query(host) {
            liaisonyy_core::clock::init_network_time(t);
            println!("network time synced via HTTP Date {host}: {t}");
            return Some(t);
        }
    }
    None
}

#[derive(Default, serde::Deserialize)]
struct ServerConfig {
    school_name: Option<String>,
    server_name: Option<String>,
    port: Option<u16>,
    data_dir: Option<String>,
}

fn spawn_discovery(api_port: u16) {
    std::thread::spawn(move || {
        let sock = match UdpSocket::bind(("0.0.0.0", 47821)) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("discovery disabled (UDP 47821): {e}");
                return;
            }
        };
        let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
        let resp = json!({
            "app": "liaisonyy",
            "protocol": PROTOCOL_VERSION,
            "version": APP_VERSION,
            "port": api_port,
            "name": "杨村一中 liaisonyy"
        })
        .to_string();
        let mut buf = [0u8; 512];
        loop {
            if let Ok((n, addr)) = sock.recv_from(&mut buf) {
                if buf[..n].starts_with(b"LIAISONYY_DISCOVER_V1") {
                    let _ = sock.send_to(resp.as_bytes(), addr);
                }
            }
        }
    });
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("data")))
            .unwrap_or_else(|| PathBuf::from("data"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("panic.txt"), format!("{info}\n"));
    }));
    if let Err(e) = run() {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("data")))
            .unwrap_or_else(|| PathBuf::from("data"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("last_error.txt"), format!("{e:?}\n"));
        #[cfg(not(windows))]
        eprintln!("liaisonyy failed: {e:?}");
    }
}

#[tokio::main]
async fn run() -> anyhow::Result<()> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .to_path_buf();
    let cfg: ServerConfig = std::fs::read_to_string(exe_dir.join("config.toml"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    let mut data = match &cfg.data_dir {
        Some(d) => {
            let p = PathBuf::from(d);
            if p.is_absolute() {
                p
            } else {
                exe_dir.join(p)
            }
        }
        None => exe_dir.join("data"),
    };
    let mut port: u16 = cfg.port.unwrap_or(47820);
    let mut install_autostart = false;
    let mut remove_autostart = false;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--data-dir" if i + 1 < args.len() => {
                data = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(47820);
                i += 2;
            }
            "--install-autostart" => {
                install_autostart = true;
                i += 1;
            }
            "--remove-autostart" => {
                remove_autostart = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if install_autostart || remove_autostart {
        #[cfg(windows)]
        {
            let appdata = std::env::var("APPDATA")?;
            let dir = PathBuf::from(appdata)
                .join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
            let p = dir.join("liaisonyy-server.vbs");
            if remove_autostart {
                let _ = std::fs::remove_file(&p);
                let _ = std::fs::remove_file(dir.join("liaisonyy-server.cmd"));
                println!("autostart removed: {}", p.display());
            } else {
                std::fs::create_dir_all(&dir)?;
                let exe = std::env::current_exe()?;
                let workdir = exe.parent().unwrap_or_else(|| FsPath::new("."));
                let script = format!(
                    "Set sh = CreateObject(\"WScript.Shell\")\r\nsh.CurrentDirectory = \"{}\"\r\nsh.Run \"\"\"{}\"\" --data-dir \"\"{}\"\" --port {}\", 0, False\r\n",
                    workdir.display(),
                    exe.display(),
                    data.display(),
                    port
                );
                std::fs::write(&p, script)?;
                let _ = std::fs::remove_file(dir.join("liaisonyy-server.cmd"));
                println!("autostart installed: {}", p.display());
            }
        }
        #[cfg(not(windows))]
        {
            eprintln!("autostart is only supported on Windows");
        }
        return Ok(());
    }
    std::fs::create_dir_all(&data)?;
    #[cfg(windows)]
    {
        let log_dir = data.join("logs");
        std::fs::create_dir_all(&log_dir)?;
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join("server.log"))
        {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("info".parse()?),
                )
                .with_writer(std::sync::Mutex::new(file))
                .try_init();
        }
    }
    #[cfg(not(windows))]
    {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("info".parse()?),
            )
            .try_init();
    }
    apply_pending_restore(&data)?;
    spawn_discovery(port);
    let conn = Connection::open(data.join("liaisonyy.db"))?;
    init_db(&conn)?;
    if let Some(v) = &cfg.school_name {
        if get_setting(&conn, "school_name")?.is_none() {
            set_setting(&conn, "school_name", v)?;
        }
    }
    if let Some(v) = &cfg.server_name {
        if get_setting(&conn, "server_name")?.is_none() {
            set_setting(&conn, "server_name", v)?;
        }
    }
    if sync_network_time().is_none() {
        tracing::warn!("network time sync failed; using local clock until next retry");
    }
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(3600));
        let _ = sync_network_time();
    });
    let has_root: i64 = conn.query_row("SELECT COUNT(*) FROM root_state", [], |r| r.get(0))?;
    let hmac = load_or_create_keyring(&data)?;
    let setup_token = load_or_create_setup_token(&data, has_root > 0)?;
    let (ws_tx, _) = tokio::sync::broadcast::channel::<()>(64);
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        data: data.clone(),
        hmac: Arc::new(hmac),
        media_key: Arc::new(load_or_create_media_key(&data)?),
        ws: ws_tx,
        rate: Arc::new(Mutex::new(HashMap::new())),
        setup_token: Arc::new(Mutex::new(setup_token.clone())),
        version: Arc::new(APP_VERSION.to_string()),
    };
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/bootstrap", get(bootstrap))
        .route("/api/v1/setup", post(setup))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/reset-fetch", post(reset_fetch))
        .route("/api/v1/auth/reset", post(reset))
        .route("/api/v1/classes", get(list_classes))
        .route("/api/v1/classes/:id/roster", get(list_roster))
        .route("/api/v1/classes/:id/accounts", get(list_accounts))
        .route("/api/v1/me", get(me).patch(update_me))
        .route("/api/v1/me/profile", get(get_my_profile).patch(update_profile))
        .route("/api/v1/me/profile-key", post(set_profile_key))
        .route("/api/v1/me/rekey-class", post(rekey_class))
        .route("/api/v1/me/avatar", post(upload_avatar))
        .route("/api/v1/me/password", post(change_password))
        .route("/api/v1/me/pin", post(set_pin))
        .route("/api/v1/me/login-policy", post(set_login_policy))
        .route("/api/v1/users/:uid", get(get_user))
        .route("/api/v1/users/:uid/profile", get(get_user_profile))
        .route("/api/v1/lookup/user", get(lookup_user))
        .route("/api/v1/files/:id", get(get_file))
        .route("/api/v1/friends/:uid", delete(delete_friend))
        .route("/api/v1/blocks", get(list_blocks).post(block_user))
        .route("/api/v1/blocks/:uid", delete(unblock_user))
        .route("/api/v1/notifications", get(list_notifications))
        .route("/api/v1/notifications/read", post(mark_notifications_read))
        .route("/api/v1/announcements", get(list_announcements))
        .route("/api/v1/reports", post(create_report))
        .route("/api/v1/admin/announcements", post(create_announcement))
        .route("/api/v1/admin/announcements/:id", delete(delete_announcement))
        .route("/api/v1/admin/reports", get(admin_reports))
        .route("/api/v1/admin/reports/:id/resolve", post(resolve_report))
        .route("/api/v1/admin/users/:uid/action", post(admin_user_action))
        .route("/api/v1/transfers", get(my_transfers).post(create_transfer))
        .route("/api/v1/admin/transfers", get(admin_transfers))
        .route("/api/v1/admin/transfers/:id/action", post(transfer_action))
        .route("/api/v1/friends/requests", get(list_requests).post(create_request))
        .route("/api/v1/friends/requests/:id/accept", post(accept_request))
        .route("/api/v1/friends/requests/:id/decline", post(decline_request))
        .route("/api/v1/friends", get(list_friends))
        .route("/api/v1/conversations", get(list_conversations))
        .route("/api/v1/conversations/:id/messages", get(list_messages).post(send_message))
        .route("/api/v1/conversations/:id/keys", get(conversation_keys))
        .route("/api/v1/conversations/:id/read", post(mark_read))
        .route("/api/v1/admin/stats", get(admin_stats))
        .route("/api/v1/admin/dashboard", get(admin_dashboard))
        .route("/api/v1/admin/audit", get(admin_audit))
        .route("/api/v1/admin/audit/verify", get(admin_audit_verify))
        .route("/api/v1/admin/audit.csv", get(admin_audit_csv))
        .route("/api/v1/admin/backups", get(list_backups).post(create_backup))
        .route("/api/v1/admin/backups/:name/restore", post(request_restore))
        .route("/api/v1/admin/users/:uid/conversations", get(admin_user_conversations))
        .route("/api/v1/admin/conversations/:id/messages", get(admin_conversation_messages))
        .route("/api/v1/admin/students/:uid/conversations/:conv_id/keys", get(admin_student_conv_keys))
        .route("/api/v1/admin/classes", get(admin_classes).post(create_class))
        .route("/api/v1/admin/classes/:id/roster", post(import_roster))
        .route("/api/v1/admin/classes/:id/admin", post(assign_class_admin))
        .route("/api/v1/admin/classes/:id/escrow", get(class_escrow))
        .route("/api/v1/admin/students/:uid/keys", get(admin_student_keys))
        .route("/api/v1/admin/reset-keys", post(create_reset_key))
        .route("/api/v1/update/manifest", get(update_manifest))
        .route("/api/v1/ws", get(ws_handler))
        .route("/api/v1/update/package", get(update_package))
        .route("/api/v1/admin/update/package", post(admin_upload_update))
        .route("/admin", get(admin_page))
        .route("/admin/", get(admin_page))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state);
    let (cert, key) = ensure_tls(&data)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    tracing::info!("liaisonyy server data: {}", data.display());
    if !setup_token.is_empty() {
        tracing::info!("SETUP TOKEN: {setup_token} (also in data/setup_token.txt)");
    }
    tracing::info!("API:   https://127.0.0.1:{port}/api/v1/health");
    tracing::info!("Admin: https://127.0.0.1:{port}/admin");
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
