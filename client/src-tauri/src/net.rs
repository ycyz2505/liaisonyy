use crate::store::{self, Cache, PendingMessage, Settings};
use liaisonyy_core::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize)]
pub struct MessageView {
    pub id: i64,
    pub sender_uid: i64,
    pub sender_name: String,
    pub text: String,
    pub created_at: i64,
    pub mine: bool,
    pub pending: bool,
    pub client_msg_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FriendRequestView {
    pub id: i64,
    pub from_uid: i64,
    pub from_name: String,
    pub to_uid: i64,
    pub to_name: String,
    pub from_public_key: String,
    pub message: String,
    pub status: String,
    pub created_at: i64,
    pub incoming: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Cmd {
    SetServer(String),
    Bootstrap,
    Setup { token: String, handle: String, display_name: String, password: String },
    Register {
        class_id: i64,
        roster_id: i64,
        real_name: String,
        handle: String,
        display_name: String,
        password: String,
        login_policy: String,
        pin: Option<String>,
    },
    Login { key: String, kind: String, input: String },
    Logout,
    Roster { class_id: i64 },
    Accounts { class_id: i64 },
    RefreshConversations,
    OpenConversation { conv_id: i64 },
    SendMessage { conv_id: i64, text: String, client_msg_id: String },
    MarkRead { conv_id: i64, last_id: i64 },
    Requests,
    AddFriend { to_uid: i64, message: String },
    AcceptFriend { request_id: i64, from_uid: i64 },
    DeclineFriend { request_id: i64 },
    Friends,
    AdminStats,
    AdminAudit,
    AdminClasses,
    CreateClass { name: String, grade: String },
    ImportRoster { class_id: i64, names: Vec<String> },
    AssignAdmin { class_id: i64, uid: i64 },
    CreateResetKey { uid: i64, expires_minutes: i64 },
    Reset { key: String, code: String, new_password: String, new_pin: Option<String> },
    UpdateMe { display_name: String },
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Ev {
    Bootstrap(BootstrapResponse),
    Roster(Vec<RosterEntry>),
    Accounts(Vec<AccountPublic>),
    SetupDone { recovery_code: String },
    LoggedIn { user: LoginResponse, offline: bool },
    LoggedOut,
    Conversations(Vec<ConversationInfo>),
    Messages { conv_id: i64, messages: Vec<MessageView>, replace: bool },
    MessageSent { conv_id: i64, client_msg_id: String, id: i64 },
    Requests(Vec<FriendRequestView>),
    Friends(Vec<FriendDto>),
    AdminStats(AdminStats),
    AdminAudit(Vec<AuditEntry>),
    AdminClasses(Vec<ClassInfo>),
    ResetKey { code: String, expires_at: i64 },
    ResetDone,
    Toast(String),
    Error(String),
    Offline(bool),
}

struct Session {
    token: String,
    uid: i64,
    role: String,
    class_id: Option<i64>,
    login_policy: String,
    public_key: String,
    sk_u: [u8; 32],
    conv_keys: HashMap<i64, HashMap<i64, [u8; 32]>>,
    root_pk: Option<String>,
    class_keys: HashMap<i64, [u8; 32]>,
}

struct Worker {
    agent: ureq::Agent,
    base: String,
    settings: Settings,
    cache: Cache,
    session: Option<Session>,
    ev: Sender<Ev>,
    last_poll: Instant,
    offline: bool,
}

pub fn spawn(settings: Settings, cache: Cache) -> (Sender<Cmd>, Receiver<Ev>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
    let (ev_tx, ev_rx) = mpsc::channel::<Ev>();
    thread::spawn(move || {
        let tls = ureq::native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("tls");
        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .build();
        let base = settings.server.clone();
        let mut w = Worker {
            agent,
            base,
            settings,
            cache,
            session: None,
            ev: ev_tx,
            last_poll: Instant::now() - Duration::from_secs(30),
            offline: false,
        };
        loop {
            match cmd_rx.recv_timeout(Duration::from_millis(250)) {
                Ok(cmd) => w.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if w.session.is_some() && w.last_poll.elapsed() >= Duration::from_secs(2) {
                w.poll();
                w.last_poll = Instant::now();
            }
        }
    });
    (cmd_tx, ev_rx)
}

impl Worker {
    fn send(&self, ev: Ev) {
        let _ = self.ev.send(ev);
    }
    fn err(&self, e: impl Into<String>) {
        self.send(Ev::Error(e.into()));
    }
    fn toast(&self, e: impl Into<String>) {
        self.send(Ev::Toast(e.into()));
    }
    fn token(&self) -> Result<String, String> {
        self.session.as_ref().map(|s| s.token.clone()).ok_or_else(|| "未登录".to_string())
    }
    fn sk_b64(&self) -> Result<String, String> {
        self.session.as_ref().map(|s| b64e(&s.sk_u)).ok_or_else(|| "未登录".to_string())
    }
    fn save_cache(&self) {
        store::save_cache(&self.cache);
    }
    fn set_offline(&mut self, v: bool) {
        if self.offline != v {
            self.offline = v;
            self.send(Ev::Offline(v));
        }
    }

    fn call(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> Result<Value, String> {
        let url = format!("{}{}", self.base.trim_end_matches('/'), path);
        let mut req = self.agent.request(method, &url);
        if let Some(t) = token {
            req = req.set("Authorization", &format!("Bearer {t}"));
        }
        let res = if let Some(b) = body { req.send_json(b) } else { req.call() };
        match res {
            Ok(resp) => resp.into_json::<Value>().map_err(|e| format!("响应解析失败: {e}")),
            Err(ureq::Error::Status(_, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                let msg = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
                    .unwrap_or(text);
                Err(msg)
            }
            Err(e) => Err(format!("网络错误: {e}")),
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::SetServer(url) => {
                self.base = url.clone();
                self.settings.server = url;
                store::save_settings(&self.settings);
                self.toast("服务器地址已保存");
            }
            Cmd::Bootstrap => match self.do_bootstrap() {
                Ok(b) => self.send(Ev::Bootstrap(b)),
                Err(e) => self.err(e),
            },
            Cmd::Setup { token, handle, display_name, password } => {
                if let Err(e) = self.do_setup(token, handle, display_name, password) {
                    self.err(e);
                }
            }
            Cmd::Register { class_id, roster_id, real_name, handle, display_name, password, login_policy, pin } => {
                if let Err(e) = self.do_register(class_id, roster_id, real_name, handle, display_name, password, login_policy, pin) {
                    self.err(e);
                }
            }
            Cmd::Login { key, kind, input } => {
                if let Err(e) = self.do_login(key, kind, input) {
                    self.err(e);
                }
            }
            Cmd::Logout => {
                if let Ok(t) = self.token() {
                    let _ = self.call("POST", "/api/v1/auth/logout", Some(&t), Some(json!({})));
                }
                self.session = None;
                self.send(Ev::LoggedOut);
            }
            Cmd::Roster { class_id } => match self.call("GET", &format!("/api/v1/classes/{class_id}/roster"), None, None) {
                Ok(v) => match serde_json::from_value::<Vec<RosterEntry>>(v) {
                    Ok(r) => self.send(Ev::Roster(r)),
                    Err(e) => self.err(e.to_string()),
                },
                Err(e) => self.err(e),
            },
            Cmd::Accounts { class_id } => match self.call("GET", &format!("/api/v1/classes/{class_id}/accounts"), None, None) {
                Ok(v) => match serde_json::from_value::<Vec<AccountPublic>>(v) {
                    Ok(a) => self.send(Ev::Accounts(a)),
                    Err(e) => self.err(e.to_string()),
                },
                Err(e) => self.err(e),
            },
            Cmd::RefreshConversations => {
                if self.session.is_none() {
                    return;
                }
                match self.load_conversations(true) {
                    Ok(c) => self.send(Ev::Conversations(c)),
                    Err(e) => self.err(e),
                }
            }
            Cmd::OpenConversation { conv_id } => {
                if self.session.is_none() {
                    return;
                }
                let _ = self.ensure_conv_keys(conv_id);
                match self.fetch_messages(conv_id, 0, true) {
                    Ok(_) => {}
                    Err(e) => self.err(e),
                }
            }
            Cmd::SendMessage { conv_id, text, client_msg_id } => {
                if let Err(e) = self.do_send(conv_id, text, client_msg_id) {
                    self.err(e);
                }
            }
            Cmd::MarkRead { conv_id, last_id } => {
                if let Ok(t) = self.token() {
                    let _ = self.call("POST", &format!("/api/v1/conversations/{conv_id}/read"), Some(&t), Some(json!({"last_read": last_id})));
                }
            }
            Cmd::Requests => match self.do_requests() {
                Ok(v) => self.send(Ev::Requests(v)),
                Err(e) => self.err(e),
            },
            Cmd::AddFriend { to_uid, message } => {
                if let Err(e) = self.do_add_friend(to_uid, message) {
                    self.err(e);
                }
            }
            Cmd::AcceptFriend { request_id, from_uid } => {
                if let Err(e) = self.do_accept_friend(request_id, from_uid) {
                    self.err(e);
                }
            }
            Cmd::DeclineFriend { request_id } => {
                if let Ok(t) = self.token() {
                    match self.call("POST", &format!("/api/v1/friends/requests/{request_id}/decline"), Some(&t), Some(json!({}))) {
                        Ok(_) => {
                            self.toast("已拒绝");
                            if let Ok(v) = self.do_requests() {
                                self.send(Ev::Requests(v));
                            }
                        }
                        Err(e) => self.err(e),
                    }
                }
            }
            Cmd::Friends => {
                if let Ok(t) = self.token() {
                    match self.call("GET", "/api/v1/friends", Some(&t), None) {
                        Ok(v) => match serde_json::from_value::<Vec<FriendDto>>(v) {
                            Ok(f) => self.send(Ev::Friends(f)),
                            Err(e) => self.err(e.to_string()),
                        },
                        Err(e) => self.err(e),
                    }
                }
            }
            Cmd::AdminStats => match self.do_get::<AdminStats>("/api/v1/admin/stats") {
                Ok(v) => self.send(Ev::AdminStats(v)),
                Err(e) => self.err(e),
            },
            Cmd::AdminAudit => match self.do_get::<Vec<AuditEntry>>("/api/v1/admin/audit") {
                Ok(v) => self.send(Ev::AdminAudit(v)),
                Err(e) => self.err(e),
            },
            Cmd::AdminClasses => match self.do_get::<Vec<ClassInfo>>("/api/v1/admin/classes") {
                Ok(v) => self.send(Ev::AdminClasses(v)),
                Err(e) => self.err(e),
            },
            Cmd::CreateClass { name, grade } => {
                if let Err(e) = self.do_create_class(name, grade) {
                    self.err(e);
                }
            }
            Cmd::ImportRoster { class_id, names } => {
                if let Err(e) = self.do_import_roster(class_id, names) {
                    self.err(e);
                }
            }
            Cmd::AssignAdmin { class_id, uid } => {
                if let Err(e) = self.do_assign_admin(class_id, uid) {
                    self.err(e);
                }
            }
            Cmd::CreateResetKey { uid, expires_minutes } => {
                if let Err(e) = self.do_create_reset_key(uid, expires_minutes) {
                    self.err(e);
                }
            }
            Cmd::Reset { key, code, new_password, new_pin } => {
                if let Err(e) = self.do_reset(key, code, new_password, new_pin) {
                    self.err(e);
                }
            }
            Cmd::UpdateMe { display_name } => {
                if let Ok(t) = self.token() {
                    match self.call("PATCH", "/api/v1/me", Some(&t), Some(json!({"display_name": display_name}))) {
                        Ok(_) => self.toast("已保存"),
                        Err(e) => self.err(e),
                    }
                }
            }
        }
    }

    fn do_get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let t = self.token()?;
        let v = self.call("GET", path, Some(&t), None)?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    fn do_bootstrap(&mut self) -> Result<BootstrapResponse, String> {
        let v = self.call("GET", "/api/v1/bootstrap", None, None)?;
        let b: BootstrapResponse = serde_json::from_value(v).map_err(|e| e.to_string())?;
        if let Some(s) = self.session.as_mut() {
            s.root_pk = b.root_public_key.clone();
        }
        Ok(b)
    }

    fn do_setup(&mut self, token: String, handle: String, display_name: String, password: String) -> Result<(), String> {
        if password.chars().count() < 8 || !password.chars().any(|c| c.is_ascii_alphabetic()) || !password.chars().any(|c| c.is_ascii_digit()) {
            return Err("密码需 8 位以上并包含字母和数字".into());
        }
        let (sk, pk) = gen_keypair();
        let salt = new_salt();
        let params = KdfParams::default();
        let (kek, _) = derive_keys(&password, &salt, &params)?;
        let sk_raw = b64d(&sk)?;
        let root_own = wrap_key(&kek, &sk_raw);
        let recovery_code = random_code(16);
        let rec_salt = b64e(b"liaisonyy-recovery-salt");
        let (rk, _) = derive_keys(&recovery_code, &rec_salt, &params)?;
        let root_recovery = wrap_key(&rk, &sk_raw);
        self.call(
            "POST",
            "/api/v1/setup",
            None,
            Some(json!({
                "setup_token": token,
                "handle": handle,
                "display_name": display_name,
                "password": password,
                "public_key": pk,
                "pwd_salt": salt,
                "pwd_params": params,
                "root_own": root_own,
                "root_recovery": root_recovery
            })),
        )?;
        self.send(Ev::SetupDone { recovery_code });
        Ok(())
    }

    fn do_register(
        &mut self,
        class_id: i64,
        roster_id: i64,
        real_name: String,
        handle: String,
        display_name: String,
        password: String,
        login_policy: String,
        pin: Option<String>,
    ) -> Result<(), String> {
        if password.chars().count() < 8 || !password.chars().any(|c| c.is_ascii_alphabetic()) || !password.chars().any(|c| c.is_ascii_digit()) {
            return Err("密码需 8 位以上并包含字母和数字".into());
        }
        let b = self.do_bootstrap()?;
        let root_pk = b.root_public_key.ok_or_else(|| "服务端尚未初始化".to_string())?;
        let escrow_pk = b
            .classes
            .iter()
            .find(|c| c.id == class_id)
            .and_then(|c| c.escrow_public_key.clone())
            .ok_or_else(|| "班级尚未初始化密钥".to_string())?;
        let (sk, pk) = gen_keypair();
        let (pdk, _) = gen_keypair();
        let pdk_raw = b64d(&pdk)?;
        let sk_raw = b64d(&sk)?;
        let salt = new_salt();
        let params = KdfParams::default();
        let (kek, _) = derive_keys(&password, &salt, &params)?;
        let own_priv = wrap_key(&kek, &sk_raw);
        let user_priv_adm = seal(&root_pk, &sk_raw)?;
        let user_priv_class = seal(&escrow_pk, &sk_raw)?;
        let profile_own = wrap_key(&kek, &pdk_raw);
        let profile_adm = seal(&root_pk, &pdk_raw)?;
        let profile_class = seal(&escrow_pk, &pdk_raw)?;
        let mut pin_salt = None;
        let mut pin_params = None;
        let mut pin_own = None;
        let mut profile_pin = None;
        if login_policy == "pin_allowed" {
            let p = pin.clone().ok_or_else(|| "缺少 PIN".to_string())?;
            if p.chars().count() < 4 || !p.chars().all(|c| c.is_ascii_digit()) {
                return Err("PIN 需 4-20 位纯数字".into());
            }
            let s = new_salt();
            let pp = KdfParams::default();
            let (pkek, _) = derive_keys(&p, &s, &pp)?;
            pin_own = Some(wrap_key(&pkek, &sk_raw));
            profile_pin = Some(wrap_key(&pkek, &pdk_raw));
            pin_salt = Some(s);
            pin_params = Some(pp);
        }
        let v = self.call(
            "POST",
            "/api/v1/auth/register",
            None,
            Some(json!({
                "class_id": class_id,
                "roster_id": roster_id,
                "real_name": real_name,
                "handle": handle,
                "display_name": display_name,
                "password": password,
                "login_policy": login_policy,
                "pin": pin,
                "public_key": pk,
                "pwd_salt": salt,
                "pwd_params": params,
                "pin_salt": pin_salt,
                "pin_params": pin_params,
                "own_priv": own_priv,
                "pin_own": pin_own,
                "user_priv_adm": user_priv_adm,
                "user_priv_class": user_priv_class,
                "profile_own": profile_own,
                "profile_pin": profile_pin,
                "profile_adm": profile_adm,
                "profile_class": profile_class
            })),
        )?;
        let resp: LoginResponse = serde_json::from_value(v).map_err(|e| e.to_string())?;
        self.finish_login(resp, "password", &password)?;
        Ok(())
    }

    fn do_login(&mut self, key: String, kind: String, input: String) -> Result<(), String> {
        let v = self.call(
            "POST",
            "/api/v1/auth/login",
            None,
            Some(json!({"uid_or_handle": key, "kind": kind, "input": input})),
        );
        match v {
            Ok(v) => {
                let resp: LoginResponse = serde_json::from_value(v).map_err(|e| e.to_string())?;
                let chars: Vec<char> = input.chars().collect();
                if resp.matched_offset + resp.matched_len > chars.len() {
                    return Err("服务端返回的匹配位置异常".into());
                }
                let secret: String = chars.iter().skip(resp.matched_offset).take(resp.matched_len).collect();
                self.finish_login(resp, &kind, &secret)?;
                Ok(())
            }
            Err(e) => {
                if e.starts_with("网络错误") {
                    if let Ok(()) = self.offline_login(&key, &kind, &input) {
                        return Ok(());
                    }
                }
                Err(e)
            }
        }
    }

    fn finish_login(&mut self, resp: LoginResponse, kind: &str, secret: &str) -> Result<(), String> {
        let (salt, params, wrapped) = if kind == "pin" {
            (
                resp.pin_salt.clone().ok_or_else(|| "该账号未设置 PIN".to_string())?,
                resp.pin_params.clone().ok_or_else(|| "PIN 参数缺失".to_string())?,
                resp.pin_own.clone().ok_or_else(|| "PIN 密钥包裹缺失".to_string())?,
            )
        } else {
            (resp.pwd_salt.clone(), resp.pwd_params.clone(), resp.own_priv.clone())
        };
        let (kek, _) = derive_keys(secret, &salt, &params)?;
        let sk = unwrap_key(&kek, &wrapped)?;
        if sk.len() != 32 {
            return Err("私钥长度异常".into());
        }
        let mut sk_u = [0u8; 32];
        sk_u.copy_from_slice(&sk);
        let uid = resp.uid;
        let role = resp.role.clone();
        let class_id = resp.class_id;
        let login_policy = resp.login_policy.clone();
        let public_key = resp.public_key.clone();
        self.session = Some(Session {
            token: resp.token.clone(),
            uid,
            role,
            class_id,
            login_policy,
            public_key,
            sk_u,
            conv_keys: HashMap::new(),
            root_pk: None,
            class_keys: HashMap::new(),
        });
        let _ = self.do_bootstrap();
        self.settings.last_uid = Some(uid);
        if resp.class_id.is_some() {
            self.settings.class_id = resp.class_id;
            if let Some(c) = resp.classes.iter().find(|c| Some(c.id) == resp.class_id) {
                self.settings.class_name = c.name.clone();
            }
        }
        store::save_settings(&self.settings);
        let mut cached = resp.clone();
        cached.token.clear();
        self.cache.login = Some(cached);
        self.save_cache();
        self.set_offline(false);
        self.send(Ev::LoggedIn { user: resp, offline: false });
        let convs = self.load_conversations(true)?;
        self.send(Ev::Conversations(convs));
        Ok(())
    }

    fn offline_login(&mut self, key: &str, kind: &str, input: &str) -> Result<(), String> {
        let cached = self.cache.login.clone().ok_or_else(|| "没有本机缓存，无法离线登录".to_string())?;
        let key_match = key.chars().all(|c| c.is_ascii_digit())
            && key.parse::<i64>().ok() == Some(cached.uid)
            || key.to_lowercase() == cached.handle.to_lowercase();
        if !key_match {
            return Err("本机没有该账号的离线缓存".into());
        }
        if kind == "pin" && cached.login_policy != "pin_allowed" {
            return Err("该账号未开启 PIN 登录".into());
        }
        let (salt, params, wrapped) = if kind == "pin" {
            (
                cached.pin_salt.clone().ok_or_else(|| "该账号未设置 PIN".to_string())?,
                cached.pin_params.clone().ok_or_else(|| "PIN 参数缺失".to_string())?,
                cached.pin_own.clone().ok_or_else(|| "PIN 密钥包裹缺失".to_string())?,
            )
        } else {
            (cached.pwd_salt.clone(), cached.pwd_params.clone(), cached.own_priv.clone())
        };
        let (kek, _) = derive_keys(input, &salt, &params).map_err(|_| "离线登录失败".to_string())?;
        let sk = unwrap_key(&kek, &wrapped).map_err(|_| "离线登录需输入完整密码/PIN".to_string())?;
        if sk.len() != 32 {
            return Err("私钥长度异常".into());
        }
        let mut sk_u = [0u8; 32];
        sk_u.copy_from_slice(&sk);
        let mut conv_keys = HashMap::new();
        for c in &self.cache.conversations {
            if let Ok(w) = open_sealed(&b64e(&sk_u), &c.member_wrap) {
                if w.len() == 32 {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&w);
                    conv_keys.entry(c.id).or_insert_with(HashMap::new).insert(c.key_version, k);
                }
            }
        }
        self.session = Some(Session {
            token: String::new(),
            uid: cached.uid,
            role: cached.role.clone(),
            class_id: cached.class_id,
            login_policy: cached.login_policy.clone(),
            public_key: cached.public_key.clone(),
            sk_u,
            conv_keys,
            root_pk: None,
            class_keys: HashMap::new(),
        });
        self.offline = true;
        self.send(Ev::Offline(true));
        self.send(Ev::LoggedIn { user: cached.clone(), offline: true });
        self.send(Ev::Conversations(self.cache.conversations.clone()));
        for c in &self.cache.conversations {
            let raw = self.cache.messages.get(&c.id.to_string()).cloned().unwrap_or_default();
            let msgs = self.decrypt_dtos(c.id, raw);
            self.send(Ev::Messages { conv_id: c.id, messages: msgs, replace: true });
        }
        Ok(())
    }

    fn ensure_conv_keys(&mut self, conv_id: i64) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/conversations/{conv_id}/keys"), Some(&t), None)?;
        let arr = v.as_array().cloned().unwrap_or_default();
        let sk = self.sk_b64()?;
        let sk_raw = b64d(&sk)?;
        if sk_raw.len() != 32 {
            return Err("私钥长度异常".into());
        }
        let mut sk_arr = [0u8; 32];
        sk_arr.copy_from_slice(&sk_raw);
        let mut map = HashMap::new();
        for item in arr {
            let ver = item.get("key_version").and_then(|x| x.as_i64()).unwrap_or(0);
            let wrapped = item.get("wrapped_key").and_then(|x| x.as_str()).unwrap_or("");
            if let Ok(w) = open_sealed(&b64e(&sk_arr), wrapped) {
                if w.len() == 32 {
                    let mut k = [0u8; 32];
                    k.copy_from_slice(&w);
                    map.insert(ver, k);
                }
            }
        }
        if !map.is_empty() {
            self.session.as_mut().unwrap().conv_keys.insert(conv_id, map);
        }
        Ok(())
    }

    fn load_conversations(&mut self, with_messages: bool) -> Result<Vec<ConversationInfo>, String> {
        let t = self.token()?;
        let v = self.call("GET", "/api/v1/conversations", Some(&t), None)?;
        let convs: Vec<ConversationInfo> = serde_json::from_value(v).map_err(|e| e.to_string())?;
        for c in &convs {
            let _ = self.ensure_conv_keys(c.id);
        }
        self.cache.conversations = convs.clone();
        self.save_cache();
        if with_messages {
            for c in &convs {
                let _ = self.fetch_messages(c.id, 0, true);
            }
        }
        Ok(convs)
    }

    fn fetch_messages(&mut self, conv_id: i64, after: i64, replace: bool) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/conversations/{conv_id}/messages?after={after}"), Some(&t), None)?;
        let dtos: Vec<MessageDto> = serde_json::from_value(v).map_err(|e| e.to_string())?;
        if dtos.is_empty() && !replace {
            return Ok(());
        }
        let max_id = dtos.iter().map(|m| m.id).max().unwrap_or(after);
        self.cache.last_ids.insert(conv_id.to_string(), max_id);
        {
            let entry = self.cache.messages.entry(conv_id.to_string()).or_default();
            for d in &dtos {
                if !entry.iter().any(|x| x.id == d.id) {
                    entry.push(d.clone());
                }
            }
            entry.sort_by_key(|m| m.id);
        }
        let views = self.decrypt_dtos(conv_id, dtos);
        self.save_cache();
        self.send(Ev::Messages { conv_id, messages: views, replace });
        Ok(())
    }

    fn decrypt_dtos(&self, conv_id: i64, dtos: Vec<MessageDto>) -> Vec<MessageView> {
        let uid = self.session.as_ref().map(|s| s.uid).unwrap_or(0);
        dtos.into_iter()
            .map(|d| {
                let text = self
                    .session
                    .as_ref()
                    .and_then(|s| s.conv_keys.get(&conv_id))
                    .and_then(|m| m.get(&d.key_version))
                    .and_then(|k| b64d(&d.ciphertext).ok().and_then(|c| aead_decrypt(k, &c, conv_id.to_string().as_bytes()).ok()))
                    .map(|v| String::from_utf8_lossy(&v).to_string())
                    .unwrap_or_else(|| "[无法解密]".into());
                MessageView {
                    id: d.id,
                    sender_uid: d.sender_uid,
                    sender_name: d.sender_name,
                    text,
                    created_at: d.created_at,
                    mine: d.sender_uid == uid,
                    pending: false,
                    client_msg_id: d.client_msg_id,
                }
            })
            .collect()
    }

    fn do_send(&mut self, conv_id: i64, text: String, client_msg_id: String) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("消息为空".into());
        }
        let version = self
            .cache
            .conversations
            .iter()
            .find(|c| c.id == conv_id)
            .map(|c| c.key_version)
            .ok_or_else(|| "会话不存在".to_string())?;
        let key = self
            .session
            .as_ref()
            .and_then(|s| s.conv_keys.get(&conv_id))
            .and_then(|m| m.get(&version))
            .copied()
            .ok_or_else(|| "会话密钥未同步，请刷新".to_string())?;
        let client_msg_id = client_msg_id;
        let ciphertext = b64e(&aead_encrypt(&key, text.as_bytes(), conv_id.to_string().as_bytes()));
        self.cache.outbox.push(PendingMessage {
            conv_id,
            key_version: version,
            ciphertext: ciphertext.clone(),
            client_msg_id: client_msg_id.clone(),
            created_at: now(),
        });
        self.save_cache();
        let t = self.token()?;
        let body = json!({"key_version": version, "ciphertext": ciphertext, "client_msg_id": client_msg_id.clone()});
        match self.call("POST", &format!("/api/v1/conversations/{conv_id}/messages"), Some(&t), Some(body)) {
            Ok(v) => {
                let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
                self.cache.outbox.retain(|p| p.client_msg_id != client_msg_id);
                self.save_cache();
                self.send(Ev::MessageSent { conv_id, client_msg_id, id });
            }
            Err(e) => {
                self.set_offline(true);
                self.toast(format!("已加入离线队列，联网后自动发送（{e}）"));
            }
        }
        Ok(())
    }

    fn replay_outbox(&mut self) {
        let t = match self.token() {
            Ok(t) if !t.is_empty() => t,
            _ => return,
        };
        let pending = self.cache.outbox.clone();
        for p in pending {
            let body = json!({"key_version": p.key_version, "ciphertext": p.ciphertext, "client_msg_id": p.client_msg_id});
            match self.call("POST", &format!("/api/v1/conversations/{}/messages", p.conv_id), Some(&t), Some(body)) {
                Ok(v) => {
                    let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
                    self.cache.outbox.retain(|x| x.client_msg_id != p.client_msg_id);
                    self.save_cache();
                    self.send(Ev::MessageSent { conv_id: p.conv_id, client_msg_id: p.client_msg_id, id });
                }
                Err(_) => break,
            }
        }
    }

    fn poll(&mut self) {
        if self.session.is_none() {
            return;
        }
        if self.token().map(|t| t.is_empty()).unwrap_or(true) {
            return;
        }
        if let Err(_) = self.call("GET", "/api/v1/health", None, None) {
            self.set_offline(true);
            return;
        }
        self.set_offline(false);
        let convs = match self.load_conversations(false) {
            Ok(c) => c,
            Err(_) => {
                self.set_offline(true);
                return;
            }
        };
        for c in &convs {
            let last = *self.cache.last_ids.get(&c.id.to_string()).unwrap_or(&0);
            let _ = self.fetch_messages(c.id, last, false);
        }
        self.send(Ev::Conversations(convs));
        self.replay_outbox();
        self.save_cache();
    }

    fn do_requests(&mut self) -> Result<Vec<FriendRequestView>, String> {
        let t = self.token()?;
        let uid = self.session.as_ref().map(|s| s.uid).unwrap_or(0);
        let sk = self.sk_b64()?;
        let sk_raw = b64d(&sk)?;
        let v = self.call("GET", "/api/v1/friends/requests", Some(&t), None)?;
        let dtos: Vec<FriendRequestDto> = serde_json::from_value(v).map_err(|e| e.to_string())?;
        Ok(dtos
            .into_iter()
            .map(|d| {
                let incoming = d.to_uid == uid;
                let message = if incoming && !d.message_enc.is_empty() {
                    open_sealed(&sk, &d.message_enc)
                        .ok()
                        .map(|v| String::from_utf8_lossy(&v).to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let _ = sk_raw;
                FriendRequestView {
                    id: d.id,
                    from_uid: d.from_uid,
                    from_name: d.from_name,
                    to_uid: d.to_uid,
                    to_name: d.to_name,
                    from_public_key: d.from_public_key,
                    message,
                    status: d.status,
                    created_at: d.created_at,
                    incoming,
                }
            })
            .collect())
    }

    fn do_add_friend(&mut self, to_uid: i64, message: String) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/users/{to_uid}"), Some(&t), None)?;
        let u: AccountPublic = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let enc = seal(&u.public_key, message.as_bytes())?;
        self.call("POST", "/api/v1/friends/requests", Some(&t), Some(json!({"to_uid": to_uid, "message_enc": enc})))?;
        self.toast("好友申请已发送");
        Ok(())
    }

    fn do_accept_friend(&mut self, request_id: i64, from_uid: i64) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/users/{from_uid}"), Some(&t), None)?;
        let u: AccountPublic = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let (dk, _) = gen_keypair();
        let dk_raw = b64d(&dk)?;
        let my_pk = self.session.as_ref().map(|s| s.public_key.clone()).ok_or_else(|| "未登录".to_string())?;
        let mut keys = vec![
            json!({"recipient_uid": self.session.as_ref().unwrap().uid, "wrapped_key": seal(&my_pk, &dk_raw)?}),
            json!({"recipient_uid": from_uid, "wrapped_key": seal(&u.public_key, &dk_raw)?}),
        ];
        if let Some(rpk) = self.session.as_ref().and_then(|s| s.root_pk.clone()) {
            keys.push(json!({"recipient_uid": 0, "wrapped_key": seal(&rpk, &dk_raw)?}));
        }
        self.call(
            "POST",
            &format!("/api/v1/friends/requests/{request_id}/accept"),
            Some(&t),
            Some(json!({"key_version": 1, "keys": keys})),
        )?;
        self.toast("已添加好友");
        if let Ok(c) = self.load_conversations(true) {
            self.send(Ev::Conversations(c));
        }
        Ok(())
    }

    fn do_create_class(&mut self, name: String, grade: String) -> Result<(), String> {
        let t = self.token()?;
        let root_pk = self.session.as_ref().and_then(|s| s.root_pk.clone()).ok_or_else(|| "缺少总管理员公钥".to_string())?;
        let (sk_c, pk_c) = gen_keypair();
        let sk_c_raw = b64d(&sk_c)?;
        let wrap = seal(&root_pk, &sk_c_raw)?;
        let v = self.call(
            "POST",
            "/api/v1/admin/classes",
            Some(&t),
            Some(json!({"name": name, "grade": grade, "escrow_public_key": pk_c, "escrow_priv_for_root": wrap})),
        )?;
        let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
        if id > 0 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&sk_c_raw);
            self.session.as_mut().unwrap().class_keys.insert(id, arr);
        }
        self.toast("班级已创建");
        if let Ok(c) = self.do_get::<Vec<ClassInfo>>("/api/v1/admin/classes") {
            self.send(Ev::AdminClasses(c));
        }
        Ok(())
    }

    fn do_import_roster(&mut self, class_id: i64, names: Vec<String>) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call(
            "POST",
            &format!("/api/v1/admin/classes/{class_id}/roster"),
            Some(&t),
            Some(json!({"names": names})),
        )?;
        let n = v.get("imported").and_then(|x| x.as_i64()).unwrap_or(0);
        self.toast(format!("已导入 {n} 人"));
        Ok(())
    }

    fn class_key(&mut self, class_id: i64) -> Result<[u8; 32], String> {
        if let Some(k) = self.session.as_ref().and_then(|s| s.class_keys.get(&class_id)) {
            return Ok(*k);
        }
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/admin/classes/{class_id}/escrow"), Some(&t), None)?;
        let wrap = v.get("escrow_priv_for_root").and_then(|x| x.as_str()).unwrap_or("");
        let sk = self.sk_b64()?;
        let raw = b64d(&sk)?;
        let k = open_sealed(&sk, wrap).map_err(|_| "无法解开班级托管密钥".to_string())?;
        if k.len() != 32 {
            return Err("班级托管密钥长度异常".into());
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&k);
        let _ = raw;
        self.session.as_mut().unwrap().class_keys.insert(class_id, arr);
        Ok(arr)
    }

    fn do_assign_admin(&mut self, class_id: i64, uid: i64) -> Result<(), String> {
        let t = self.token()?;
        let sk_c = self.class_key(class_id)?;
        let v = self.call("GET", &format!("/api/v1/users/{uid}"), Some(&t), None)?;
        let u: AccountPublic = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let wrap = seal(&u.public_key, &sk_c)?;
        self.call(
            "POST",
            &format!("/api/v1/admin/classes/{class_id}/admin"),
            Some(&t),
            Some(json!({"uid": uid, "class_priv_for_admin": wrap})),
        )?;
        self.toast("已设为班级管理员");
        Ok(())
    }

    fn do_create_reset_key(&mut self, uid: i64, expires_minutes: i64) -> Result<(), String> {
        let t = self.token()?;
        let v = self.call("GET", &format!("/api/v1/admin/students/{uid}/keys"), Some(&t), None)?;
        let mode = v.get("mode").and_then(|x| x.as_str()).unwrap_or("");
        let user_priv = v.get("user_priv").and_then(|x| x.as_str()).unwrap_or("");
        let profile_wrap = v.get("profile_wrap").and_then(|x| x.as_str()).unwrap_or("");
        let class_wrap = v.get("class_wrap").and_then(|x| x.as_str()).unwrap_or("");
        let sk = self.sk_b64()?;
        let sk_raw = b64d(&sk)?;
        let (student_sk, pdk) = if mode == "root" {
            (open_sealed(&sk, user_priv)?, open_sealed(&sk, profile_wrap)?)
        } else {
            let sk_c = open_sealed(&sk, class_wrap)?;
            let sk_c_b64 = b64e(&sk_c);
            (open_sealed(&sk_c_b64, user_priv)?, open_sealed(&sk_c_b64, profile_wrap)?)
        };
        let _ = sk_raw;
        let code = random_code(8);
        let salt = new_salt();
        let params = KdfParams::default();
        let (rk, _) = derive_keys(&code, &salt, &params)?;
        let reset_wrap_priv = b64e(&aead_encrypt(&rk, &student_sk, b"liaisonyy-reset-v1"));
        let reset_wrap_profile = b64e(&aead_encrypt(&rk, &pdk, b"liaisonyy-reset-v1"));
        let root_pk = self.session.as_ref().and_then(|s| s.root_pk.clone()).ok_or_else(|| "缺少总管理员公钥".to_string())?;
        let code_wrap_root = seal(&root_pk, code.as_bytes())?;
        let expires_at = now() + expires_minutes * 60;
        self.call(
            "POST",
            "/api/v1/admin/reset-keys",
            Some(&t),
            Some(json!({
                "uid": uid, "code": code, "expires_at": expires_at, "reset_salt": salt,
                "code_wrap_root": code_wrap_root, "reset_wrap_priv": reset_wrap_priv,
                "reset_wrap_profile": reset_wrap_profile
            })),
        )?;
        self.send(Ev::ResetKey { code, expires_at });
        Ok(())
    }

    fn do_reset(&mut self, key: String, code: String, new_password: String, new_pin: Option<String>) -> Result<(), String> {
        if new_password.chars().count() < 8 || !new_password.chars().any(|c| c.is_ascii_alphabetic()) || !new_password.chars().any(|c| c.is_ascii_digit()) {
            return Err("新密码需 8 位以上并包含字母和数字".into());
        }
        let v = self.call(
            "POST",
            "/api/v1/auth/reset-fetch",
            None,
            Some(json!({"uid_or_handle": key, "code": code})),
        )?;
        let r: ResetFetchResponse = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let params = KdfParams::default();
        let (rk, _) = derive_keys(&code, &r.reset_salt, &params)?;
        let student_sk = aead_decrypt(&rk, &b64d(&r.reset_wrap_priv)?, b"liaisonyy-reset-v1")?;
        let pdk = aead_decrypt(&rk, &b64d(&r.reset_wrap_profile)?, b"liaisonyy-reset-v1")?;
        let salt = new_salt();
        let (kek, _) = derive_keys(&new_password, &salt, &params)?;
        let own_priv = wrap_key(&kek, &student_sk);
        let profile_own = wrap_key(&kek, &pdk);
        let (pin_salt, pin_params, pin_own, profile_pin) = if let Some(pin) = new_pin.clone() {
            if pin.chars().count() < 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
                return Err("PIN 需 4-20 位纯数字".into());
            }
            let ps = new_salt();
            let (pk, _) = derive_keys(&pin, &ps, &params)?;
            (Some(ps), Some(params.clone()), Some(wrap_key(&pk, &student_sk)), Some(wrap_key(&pk, &pdk)))
        } else {
            (None, None, None, None)
        };
        self.call(
            "POST",
            "/api/v1/auth/reset",
            None,
            Some(json!({
                "uid_or_handle": key, "code": code, "new_password": new_password, "new_pin": new_pin,
                "pwd_salt": salt, "pwd_params": params,
                "own_priv": own_priv, "pin_own": pin_own, "pin_salt": pin_salt, "pin_params": pin_params,
                "profile_own": profile_own, "profile_pin": profile_pin
            })),
        )?;
        self.send(Ev::ResetDone);
        Ok(())
    }
}
