use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use liaisonyy_core::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
};

const ADMIN_HTML: &str = include_str!("../../../admin-web/index.html");

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    data: PathBuf,
    hmac: Arc<Vec<u8>>,
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
            "SELECT s.uid, u.role, u.class_id FROM sessions s JOIN users u ON u.uid=s.uid WHERE s.token_hmac=?1 AND s.revoked=0 AND s.expires_at>?2",
            params![h, now()],
            |r| Ok(AuthUser { uid: r.get(0)?, role: r.get(1)?, class_id: r.get(2)? }),
        )
        .optional()?;
    row.ok_or_else(|| AppError::unauthorized("登录已过期"))
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
    let (handle, display_name, real_name, class_id, role, login_policy, public_key) = conn.query_row(
        "SELECT handle_display,display_name,real_name,class_id,role,login_policy,public_key FROM users WHERE uid=?1",
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
    Json(json!({"status":"ok","version":state.version.as_str(),"protocol":PROTOCOL_VERSION}))
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
        "SELECT uid,handle_display,display_name,class_id,role,login_policy,public_key FROM users WHERE class_id=?1 AND status='active' ORDER BY uid",
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
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (uid, handle, display_name, class_id, role, login_policy, public_key) = row?;
        let unread: i64 = conn.query_row(
            "SELECT COALESCE(SUM((SELECT COUNT(*) FROM messages m WHERE m.conv_id=cm.conv_id AND m.id>cm.last_read AND m.sender_uid!=?1)),0) FROM conversation_members cm WHERE cm.uid=?1",
            params![uid],
            |r| r.get(0),
        )?;
        out.push(AccountPublic { uid, handle, display_name, class_id, role, login_policy, public_key, unread });
    }
    Ok(Json(out))
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<MeResponse>> {
    let a = auth(&state, &headers)?;
    let conn = state.db.lock().unwrap();
    let (handle, display_name, real_name, class_id, role, login_policy, public_key) = conn.query_row(
        "SELECT handle_display,display_name,real_name,class_id,role,login_policy,public_key FROM users WHERE uid=?1",
        params![a.uid],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
    )?;
    Ok(Json(MeResponse { uid: a.uid, handle, display_name, real_name, class_id, role, login_policy, public_key }))
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
    let (handle, display_name, class_id, role, login_policy, public_key) = conn.query_row(
        "SELECT handle_display,display_name,class_id,role,login_policy,public_key FROM users WHERE uid=?1",
        params![uid],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?)),
    ).optional()?.ok_or_else(|| AppError::not_found("用户不存在"))?;
    Ok(Json(AccountPublic { uid, handle, display_name, class_id: class_id.unwrap_or(0), role, login_policy, public_key, unread: 0 }))
}

async fn create_request(State(state): State<AppState>, headers: HeaderMap, Json(req): Json<FriendRequestCreate>) -> ApiResult<Json<Value>> {
    let a = auth(&state, &headers)?;
    if req.to_uid == a.uid {
        return Err(AppError::bad("不能添加自己"));
    }
    let conn = state.db.lock().unwrap();
    let target: Option<i64> = conn.query_row("SELECT uid FROM users WHERE uid=?1 AND status='active'", params![req.to_uid], |r| r.get(0)).optional()?;
    if target.is_none() {
        return Err(AppError::not_found("用户不存在"));
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
    audit(&conn, &state.hmac, Some(a.uid), "friend.request", Some("uid"), Some(&req.to_uid.to_string()), None)?;
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
    audit(&tx, &state.hmac, Some(a.uid), "friend.accept", Some("uid"), Some(&from_uid.to_string()), None)?;
    tx.commit()?;
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
        "SELECT u.uid,u.handle_display,u.display_name,u.public_key FROM friends f JOIN users u ON u.uid=f.friend_uid WHERE f.uid=?1 ORDER BY u.display_name",
    )?;
    let rows = stmt.query_map(params![a.uid], |r| {
        Ok(FriendDto { uid: r.get(0)?, handle: r.get(1)?, display_name: r.get(2)?, public_key: r.get(3)? })
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
    let conn = state.db.lock().unwrap();
    let member: i64 = conn.query_row(
        "SELECT COUNT(*) FROM conversation_members WHERE conv_id=?1 AND uid=?2",
        params![conv_id, a.uid],
        |r| r.get(0),
    )?;
    if member == 0 {
        return Err(AppError::forbidden("不在该会话中"));
    }
    let _ = conn.query_row(
        "SELECT kind,current_key_version FROM conversations WHERE id=?1",
        params![conv_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO messages (conv_id,sender_uid,key_version,client_msg_id,ciphertext,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![conv_id, a.uid, req.key_version, req.client_msg_id, req.ciphertext, now()],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM messages WHERE sender_uid=?1 AND client_msg_id=?2",
        params![a.uid, req.client_msg_id],
        |r| r.get(0),
    )?;
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
    Ok(Json(AdminStats { users, online, messages_today, messages_total, classes, pending_requests: pending, db_bytes }))
}

async fn admin_audit(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Vec<AuditEntry>>> {
    let a = auth(&state, &headers)?;
    if a.role != "root" {
        return Err(AppError::forbidden("需要总管理员权限"));
    }
    let conn = state.db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id,ts,actor_uid,action,target_type,target_id,detail FROM audit_log ORDER BY id DESC LIMIT 200")?;
    let rows = stmt.query_map([], |r| {
        Ok(AuditEntry { id: r.get(0)?, ts: r.get(1)?, actor_uid: r.get(2)?, action: r.get(3)?, target_type: r.get(4)?, target_id: r.get(5)?, detail: r.get(6)? })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(Json(out))
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
    Json(json!({
        "schema": 1,
        "version": state.version.as_str(),
        "min_client": state.version.as_str(),
        "files": {}
    }))
}

async fn admin_page() -> Html<&'static str> {
    Html(ADMIN_HTML)
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
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conv_id,id);
CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_log(ts);
"#;

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(())
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let mut data = std::env::current_exe()?
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .join("data");
    let mut port: u16 = 47820;
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
            _ => i += 1,
        }
    }
    std::fs::create_dir_all(&data)?;
    let conn = Connection::open(data.join("liaisonyy.db"))?;
    init_db(&conn)?;
    let has_root: i64 = conn.query_row("SELECT COUNT(*) FROM root_state", [], |r| r.get(0))?;
    let hmac = load_or_create_keyring(&data)?;
    let setup_token = load_or_create_setup_token(&data, has_root > 0)?;
    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        data: data.clone(),
        hmac: Arc::new(hmac),
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
        .route("/api/v1/users/:uid", get(get_user))
        .route("/api/v1/friends/requests", get(list_requests).post(create_request))
        .route("/api/v1/friends/requests/:id/accept", post(accept_request))
        .route("/api/v1/friends/requests/:id/decline", post(decline_request))
        .route("/api/v1/friends", get(list_friends))
        .route("/api/v1/conversations", get(list_conversations))
        .route("/api/v1/conversations/:id/messages", get(list_messages).post(send_message))
        .route("/api/v1/conversations/:id/keys", get(conversation_keys))
        .route("/api/v1/conversations/:id/read", post(mark_read))
        .route("/api/v1/admin/stats", get(admin_stats))
        .route("/api/v1/admin/audit", get(admin_audit))
        .route("/api/v1/admin/classes", get(admin_classes).post(create_class))
        .route("/api/v1/admin/classes/:id/roster", post(import_roster))
        .route("/api/v1/admin/classes/:id/admin", post(assign_class_admin))
        .route("/api/v1/admin/classes/:id/escrow", get(class_escrow))
        .route("/api/v1/admin/students/:uid/keys", get(admin_student_keys))
        .route("/api/v1/admin/reset-keys", post(create_reset_key))
        .route("/api/v1/update/manifest", get(update_manifest))
        .route("/admin", get(admin_page))
        .route("/admin/", get(admin_page))
        .with_state(state);
    let (cert, key) = ensure_tls(&data)?;
    let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    println!("liaisonyy server data: {}", data.display());
    if !setup_token.is_empty() {
        println!("SETUP TOKEN: {setup_token}");
    }
    println!("API:   https://127.0.0.1:{port}/api/v1/health");
    println!("Admin: https://127.0.0.1:{port}/admin");
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
