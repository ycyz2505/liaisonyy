use liaisonyy_core::*;
use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::Duration;

fn agent() -> ureq::Agent {
    let tls = ureq::native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(tls))
        .build()
}

fn call(agent: &ureq::Agent, method: &str, url: &str, token: Option<&str>, body: Option<Value>) -> Value {
    let mut req = agent.request(method, url);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let res = if let Some(b) = body { req.send_json(b) } else { req.call() };
    match res {
        Ok(r) => r.into_json::<Value>().unwrap_or(Value::Null),
        Err(ureq::Error::Status(code, r)) => {
            let allow = r.header("allow").unwrap_or("").to_string();
            let text = r.into_string().unwrap_or_default();
            panic!("HTTP {code} {method} {url} allow={allow:?}: {text}");
        }
        Err(e) => panic!("network {url}: {e}"),
    }
}

fn get(agent: &ureq::Agent, url: &str, token: Option<&str>) -> Value {
    call(agent, "GET", url, token, None)
}
fn post(agent: &ureq::Agent, url: &str, token: Option<&str>, body: Value) -> Value {
    call(agent, "POST", url, token, Some(body))
}

fn call_status(agent: &ureq::Agent, method: &str, url: &str, token: Option<&str>, body: Option<Value>) -> (u16, Value) {
    let mut req = agent.request(method, url);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let res = if let Some(b) = body { req.send_json(b) } else { req.call() };
    match res {
        Ok(r) => (r.status(), r.into_json::<Value>().unwrap_or(Value::Null)),
        Err(ureq::Error::Status(code, r)) => {
            let v = r.into_json::<Value>().unwrap_or(Value::Null);
            (code, v)
        }
        Err(e) => panic!("network {url}: {e}"),
    }
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> (u16, String, Vec<u8>) {
    match agent.get(url).call() {
        Ok(r) => {
            let status = r.status();
            let mime = r.header("Content-Type").unwrap_or("").to_string();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut r.into_reader(), &mut buf).unwrap();
            (status, mime, buf)
        }
        Err(ureq::Error::Status(code, r)) => {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut r.into_reader(), &mut buf);
            (code, String::new(), buf)
        }
        Err(e) => panic!("network {url}: {e}"),
    }
}

struct Reg {
    resp: Value,
    sk: [u8; 32],
    pdk: [u8; 32],
    pk: String,
    uid: i64,
    token: String,
}

#[allow(clippy::too_many_arguments)]
fn register(
    agent: &ureq::Agent,
    base: &str,
    class_id: i64,
    roster_id: i64,
    real_name: &str,
    handle: &str,
    password: &str,
    pk_c: &str,
    pk_r: &str,
) -> Reg {
    let (sk, pk) = gen_keypair();
    let (pdk, _) = gen_keypair();
    let sk_raw = b64d(&sk).unwrap();
    let pdk_raw = b64d(&pdk).unwrap();
    let salt = new_salt();
    let params = KdfParams::default();
    let (kek, _) = derive_keys(password, &salt, &params).unwrap();
    let body = json!({
        "class_id": class_id,
        "roster_id": roster_id,
        "real_name": real_name,
        "handle": handle,
        "display_name": real_name,
        "password": password,
        "login_policy": "password_only",
        "pin": null,
        "public_key": pk,
        "pwd_salt": salt,
        "pwd_params": params,
        "pin_salt": null,
        "pin_params": null,
        "own_priv": wrap_key(&kek, &sk_raw),
        "pin_own": null,
        "user_priv_adm": seal(pk_r, &sk_raw).unwrap(),
        "user_priv_class": seal(pk_c, &sk_raw).unwrap(),
        "profile_own": wrap_key(&kek, &pdk_raw),
        "profile_pin": null,
        "profile_adm": seal(pk_r, &pdk_raw).unwrap(),
        "profile_class": seal(pk_c, &pdk_raw).unwrap(),
    });
    let resp = post(agent, &format!("{base}/api/v1/auth/register"), None, body);
    let uid = resp["uid"].as_i64().unwrap();
    let token = resp["token"].as_str().unwrap().to_string();
    let mut sk_arr = [0u8; 32];
    sk_arr.copy_from_slice(&sk_raw);
    let mut pdk_arr = [0u8; 32];
    pdk_arr.copy_from_slice(&pdk_raw);
    Reg { resp, sk: sk_arr, pdk: pdk_arr, pk, uid, token }
}

struct ServerGuard(std::process::Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn e2e_core_flow() {
    let dir = std::env::temp_dir().join(format!("liaisonyy-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut port = 0u16;
    for p in 48010..48100u16 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() {
            port = p;
            break;
        }
    }
    assert!(port > 0, "no free port");
    let exe = env!("CARGO_BIN_EXE_liaisonyy-server");
    let _guard = ServerGuard(
        Command::new(exe)
            .args(["--data-dir", dir.to_str().unwrap(), "--port", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let base = format!("https://127.0.0.1:{port}");
    let a = agent();
    let mut ready = false;
    for _ in 0..80 {
        if a.get(&format!("{base}/api/v1/health")).call().is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    if !ready {
        panic!("server did not start");
    }
    let setup_token = std::fs::read_to_string(dir.join("setup_token.txt")).unwrap().trim().to_string();

    // root keys + setup
    let (sk_r, pk_r) = gen_keypair();
    let salt = new_salt();
    let params = KdfParams::default();
    let (kek, _) = derive_keys("RootPass123", &salt, &params).unwrap();
    let root_own = wrap_key(&kek, &b64d(&sk_r).unwrap());
    let rec = random_code(16);
    let (rk, _) = derive_keys(&rec, &b64e(b"liaisonyy-recovery-salt"), &params).unwrap();
    let root_recovery = wrap_key(&rk, &b64d(&sk_r).unwrap());
    post(
        &a,
        &format!("{base}/api/v1/setup"),
        None,
        json!({
            "setup_token": setup_token,
            "handle": "root",
            "display_name": "总管理员",
            "password": "RootPass123",
            "public_key": pk_r,
            "pwd_salt": salt,
            "pwd_params": params,
            "root_own": root_own,
            "root_recovery": root_recovery
        }),
    );
    let login = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"root","kind":"password","input":"RootPass123"}),
    );
    let root_token = login["token"].as_str().unwrap().to_string();

    // class + roster
    let (sk_c, pk_c) = gen_keypair();
    let escrow = seal(&pk_r, &b64d(&sk_c).unwrap()).unwrap();
    let cls = post(
        &a,
        &format!("{base}/api/v1/admin/classes"),
        Some(&root_token),
        json!({"name":"测试班","grade":"高一","escrow_public_key":pk_c,"escrow_priv_for_root":escrow}),
    );
    let class_id = cls["id"].as_i64().unwrap();
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class_id}/roster"),
        Some(&root_token),
        json!({"names":["张三","李四"]}),
    );
    let roster = get(&a, &format!("{base}/api/v1/classes/{class_id}/roster"), None);
    let zhang_id = roster.as_array().unwrap().iter().find(|r| r["real_name"] == "张三").unwrap()["id"].as_i64().unwrap();
    let li_id = roster.as_array().unwrap().iter().find(|r| r["real_name"] == "李四").unwrap()["id"].as_i64().unwrap();

    let ra = register(&a, &base, class_id, zhang_id, "张三", "zhangsan", "Pass1234a", &pk_c, &pk_r);
    let rb = register(&a, &base, class_id, li_id, "李四", "lisi", "Pass1234b", &pk_c, &pk_r);

    // friend request + accept
    let enc = seal(&rb.pk, b"hi").unwrap();
    post(
        &a,
        &format!("{base}/api/v1/friends/requests"),
        Some(&ra.token),
        json!({"to_uid": rb.uid, "message_enc": enc}),
    );
    let reqs = get(&a, &format!("{base}/api/v1/friends/requests"), Some(&rb.token));
    let rid = reqs.as_array().unwrap().iter().find(|r| r["from_uid"] == ra.uid).unwrap()["id"].as_i64().unwrap();
    let (dk, _) = gen_keypair();
    let dek_raw = b64d(&dk).unwrap();
    let keys = json!([
        {"recipient_uid": ra.uid, "wrapped_key": seal(&ra.pk, &dek_raw).unwrap()},
        {"recipient_uid": rb.uid, "wrapped_key": seal(&rb.pk, &dek_raw).unwrap()},
        {"recipient_uid": 0, "wrapped_key": seal(&pk_r, &dek_raw).unwrap()}
    ]);
    let acc = post(
        &a,
        &format!("{base}/api/v1/friends/requests/{rid}/accept"),
        Some(&rb.token),
        json!({"key_version": 1, "keys": keys}),
    );
    let conv_id = acc["conv_id"].as_i64().unwrap();

    // A sends, B decrypts
    let ct = b64e(&aead_encrypt(&{
        let mut k = [0u8; 32];
        k.copy_from_slice(&dek_raw);
        k
    }, b"hello", conv_id.to_string().as_bytes()));
    post(
        &a,
        &format!("{base}/api/v1/conversations/{conv_id}/messages"),
        Some(&ra.token),
        json!({"key_version":1,"ciphertext":ct,"client_msg_id":uuid()}),
    );
    let keys_b = get(&a, &format!("{base}/api/v1/conversations/{conv_id}/keys"), Some(&rb.token));
    let wrapped_b = keys_b.as_array().unwrap().iter().find(|k| k["key_version"] == 1).unwrap()["wrapped_key"].as_str().unwrap();
    let dek_b = open_sealed(&b64e(&rb.sk), wrapped_b).unwrap();
    let mut dek_b_arr = [0u8; 32];
    dek_b_arr.copy_from_slice(&dek_b);
    let msgs = get(&a, &format!("{base}/api/v1/conversations/{conv_id}/messages?after=0"), Some(&rb.token));
    let msg = msgs.as_array().unwrap().iter().find(|m| m["sender_uid"] == ra.uid).unwrap();
    let pt = aead_decrypt(&dek_b_arr, &b64d(msg["ciphertext"].as_str().unwrap()).unwrap(), conv_id.to_string().as_bytes()).unwrap();
    assert_eq!(String::from_utf8(pt).unwrap(), "hello");

    // class group
    let convs = get(&a, &format!("{base}/api/v1/conversations"), Some(&ra.token));
    let cg = convs.as_array().unwrap().iter().find(|c| c["kind"] == "class_group").unwrap();
    let cg_id = cg["id"].as_i64().unwrap();
    let ver = cg["key_version"].as_i64().unwrap();
    let cg_keys = get(&a, &format!("{base}/api/v1/conversations/{cg_id}/keys"), Some(&ra.token));
    let wrapped_a = cg_keys.as_array().unwrap().iter().find(|k| k["key_version"] == ver).unwrap()["wrapped_key"].as_str().unwrap();
    let gk = open_sealed(&b64e(&ra.sk), wrapped_a).unwrap();
    let mut gk_arr = [0u8; 32];
    gk_arr.copy_from_slice(&gk);
    let cg_ct = b64e(&aead_encrypt(&gk_arr, b"class hello", cg_id.to_string().as_bytes()));
    post(
        &a,
        &format!("{base}/api/v1/conversations/{cg_id}/messages"),
        Some(&ra.token),
        json!({"key_version": ver, "ciphertext": cg_ct, "client_msg_id": uuid()}),
    );
    let cg_keys_b = get(&a, &format!("{base}/api/v1/conversations/{cg_id}/keys"), Some(&rb.token));
    let wrapped_b2 = cg_keys_b.as_array().unwrap().iter().find(|k| k["key_version"] == ver).unwrap()["wrapped_key"].as_str().unwrap();
    let gk_b = open_sealed(&b64e(&rb.sk), wrapped_b2).unwrap();
    let mut gk_b_arr = [0u8; 32];
    gk_b_arr.copy_from_slice(&gk_b);
    let cg_msgs = get(&a, &format!("{base}/api/v1/conversations/{cg_id}/messages?after=0"), Some(&rb.token));
    let cg_msg = cg_msgs.as_array().unwrap().iter().find(|m| m["sender_uid"] == ra.uid).unwrap();
    let cg_msg_id = cg_msg["id"].as_i64().unwrap();
    let cg_pt = aead_decrypt(&gk_b_arr, &b64d(cg_msg["ciphertext"].as_str().unwrap()).unwrap(), cg_id.to_string().as_bytes()).unwrap();
    assert_eq!(String::from_utf8(cg_pt).unwrap(), "class hello");

    // profile: private fields stay encrypted, public copy is empty when hidden
    let aad = format!("liaisonyy-profile-v1:{}", ra.uid);
    let profile_enc = b64e(&aead_encrypt(
        &ra.pdk,
        br#"{"gender":"female","birthday":"2008-06-01","socials":"QQ 123"}"#,
        aad.as_bytes(),
    ));
    call(
        &a,
        "PATCH",
        &format!("{base}/api/v1/me/profile"),
        Some(&ra.token),
        Some(json!({
            "display_name": "张三",
            "signature": "hello",
            "bio": "简介",
            "public_gender": "",
            "public_birthday": "",
            "public_socials": "",
            "profile_enc": profile_enc,
        })),
    );
    let mine = get(&a, &format!("{base}/api/v1/me/profile"), Some(&ra.token));
    assert_eq!(mine["signature"], "hello");
    let dec = aead_decrypt(
        &ra.pdk,
        &b64d(mine["profile_enc"].as_str().unwrap()).unwrap(),
        aad.as_bytes(),
    )
    .unwrap();
    assert!(String::from_utf8(dec).unwrap().contains("female"));

    let pub_a = get(&a, &format!("{base}/api/v1/users/{}/profile", ra.uid), Some(&rb.token));
    assert_eq!(pub_a["signature"], "hello");
    assert_eq!(pub_a["is_friend"], true);
    assert!(pub_a["gender"].is_null());
    assert!(pub_a["socials_json"].is_null());

    let found = get(&a, &format!("{base}/api/v1/lookup/user?q=zhangsan"), Some(&rb.token));
    assert_eq!(found["uid"].as_i64().unwrap(), ra.uid);

    // avatar upload and encrypted media download
    let png = b64d("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==").unwrap();
    let up = post(
        &a,
        &format!("{base}/api/v1/me/avatar"),
        Some(&ra.token),
        json!({"data": format!("data:image/png;base64,{}", b64e(&png)), "kind": "avatar"}),
    );
    let file_id = up["file_id"].as_i64().unwrap();
    assert!(file_id > 0);
    let (st, mime, bytes) = get_bytes(&a, &format!("{base}/api/v1/files/{file_id}"));
    assert_eq!(st, 200);
    assert_eq!(mime, "image/png");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

    // change password; the existing key wraps are re-wrapped under the new KEK
    let nsalt = new_salt();
    let (kek2, _) = derive_keys("NewPass123a", &nsalt, &params).unwrap();
    post(
        &a,
        &format!("{base}/api/v1/me/password"),
        Some(&ra.token),
        json!({
            "old_input": "Pass1234a",
            "new_password": "NewPass123a",
            "pwd_salt": nsalt,
            "pwd_params": params,
            "own_priv": wrap_key(&kek2, &ra.sk),
            "profile_own": wrap_key(&kek2, &ra.pdk),
        }),
    );
    let login2 = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"zhangsan","kind":"password","input":"NewPass123a"}),
    );
    let sk2 = unwrap_key(&kek2, login2["own_priv"].as_str().unwrap()).unwrap();
    assert_eq!(sk2, ra.sk.to_vec());

    // set PIN, then PIN login works
    let psalt = new_salt();
    let (pkek, _) = derive_keys("246810", &psalt, &params).unwrap();
    post(
        &a,
        &format!("{base}/api/v1/me/pin"),
        Some(&ra.token),
        json!({
            "password_input": "NewPass123a",
            "pin": "246810",
            "pin_salt": psalt,
            "pin_params": params,
            "pin_own": wrap_key(&pkek, &ra.sk),
            "profile_pin": wrap_key(&pkek, &ra.pdk),
        }),
    );
    let pin_login = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"zhangsan","kind":"pin","input":"246810"}),
    );
    assert!(pin_login["token"].as_str().is_some());

    // switch to password-only, PIN login is rejected
    post(
        &a,
        &format!("{base}/api/v1/me/login-policy"),
        Some(&ra.token),
        json!({"password_input": "NewPass123a", "policy": "password_only"}),
    );
    let (code, _) = call_status(
        &a,
        "POST",
        &format!("{base}/api/v1/auth/login"),
        None,
        Some(json!({"uid_or_handle":"zhangsan","kind":"pin","input":"246810"})),
    );
    assert!(code == 401 || code == 403, "pin login should fail, got {code}");

    // blocks stop direct messages in both directions
    let mut dek_arr = [0u8; 32];
    dek_arr.copy_from_slice(&dek_raw);
    post(&a, &format!("{base}/api/v1/blocks"), Some(&rb.token), json!({"uid": ra.uid}));
    let ct2 = b64e(&aead_encrypt(&dek_arr, b"blocked", conv_id.to_string().as_bytes()));
    let (code, _) = call_status(
        &a,
        "POST",
        &format!("{base}/api/v1/conversations/{conv_id}/messages"),
        Some(&ra.token),
        Some(json!({"key_version":1,"ciphertext":ct2,"client_msg_id":uuid()})),
    );
    assert_eq!(code, 403);
    call(&a, "DELETE", &format!("{base}/api/v1/blocks/{}", ra.uid), Some(&rb.token), None);
    let ct3 = b64e(&aead_encrypt(&dek_arr, b"unblocked", conv_id.to_string().as_bytes()));
    post(
        &a,
        &format!("{base}/api/v1/conversations/{conv_id}/messages"),
        Some(&ra.token),
        json!({"key_version":1,"ciphertext":ct3,"client_msg_id":uuid()}),
    );
    let blocks = get(&a, &format!("{base}/api/v1/blocks"), Some(&rb.token));
    assert!(blocks.as_array().unwrap().is_empty());

    // delete friend removes both directions
    call(&a, "DELETE", &format!("{base}/api/v1/friends/{}", ra.uid), Some(&rb.token), None);
    let friends_a = get(&a, &format!("{base}/api/v1/friends"), Some(&ra.token));
    assert!(friends_a.as_array().unwrap().is_empty());

    // report a message, admin hides it
    let rep = post(
        &a,
        &format!("{base}/api/v1/reports"),
        Some(&rb.token),
        json!({"target_type": "message", "target_id": cg_msg_id, "reason": "spam"}),
    );
    let report_id = rep["id"].as_i64().unwrap();
    let reports = get(&a, &format!("{base}/api/v1/admin/reports"), Some(&root_token));
    assert!(reports
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"].as_i64() == Some(report_id)));
    post(
        &a,
        &format!("{base}/api/v1/admin/reports/{report_id}/resolve"),
        Some(&root_token),
        json!({"action": "hide_message", "minutes": null}),
    );
    let after_hide = get(&a, &format!("{base}/api/v1/conversations/{cg_id}/messages?after=0"), Some(&ra.token));
    assert!(!after_hide
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"].as_i64() == Some(cg_msg_id)));

    // announcement reaches the class and creates a notification
    post(
        &a,
        &format!("{base}/api/v1/admin/announcements"),
        Some(&root_token),
        json!({"scope": "class", "class_id": class_id, "title": "测试公告", "body": "内容"}),
    );
    let anns = get(&a, &format!("{base}/api/v1/announcements"), Some(&ra.token));
    assert!(anns.as_array().unwrap().iter().any(|x| x["title"] == "测试公告"));
    let notifs = get(&a, &format!("{base}/api/v1/notifications"), Some(&ra.token));
    assert!(notifs.as_array().unwrap().iter().any(|n| n["kind"] == "announcement"));

    // mute blocks sending, unmute restores it
    post(
        &a,
        &format!("{base}/api/v1/admin/users/{}/action", ra.uid),
        Some(&root_token),
        json!({"action": "mute", "minutes": 10}),
    );
    let ct4 = b64e(&aead_encrypt(&dek_arr, b"muted", conv_id.to_string().as_bytes()));
    let (code, _) = call_status(
        &a,
        "POST",
        &format!("{base}/api/v1/conversations/{conv_id}/messages"),
        Some(&ra.token),
        Some(json!({"key_version":1,"ciphertext":ct4,"client_msg_id":uuid()})),
    );
    assert_eq!(code, 403);
    post(
        &a,
        &format!("{base}/api/v1/admin/users/{}/action", ra.uid),
        Some(&root_token),
        json!({"action": "unmute", "minutes": null}),
    );
    let ct5 = b64e(&aead_encrypt(&dek_arr, b"after mute", conv_id.to_string().as_bytes()));
    post(
        &a,
        &format!("{base}/api/v1/conversations/{conv_id}/messages"),
        Some(&ra.token),
        json!({"key_version":1,"ciphertext":ct5,"client_msg_id":uuid()}),
    );

    // ban revokes login; unban restores it
    post(
        &a,
        &format!("{base}/api/v1/admin/users/{}/action", ra.uid),
        Some(&root_token),
        json!({"action": "ban", "minutes": null}),
    );
    let (code, _) = call_status(
        &a,
        "POST",
        &format!("{base}/api/v1/auth/login"),
        None,
        Some(json!({"uid_or_handle":"zhangsan","kind":"password","input":"NewPass123a"})),
    );
    assert!(code == 401 || code == 403, "banned login should fail, got {code}");
    post(
        &a,
        &format!("{base}/api/v1/admin/users/{}/action", ra.uid),
        Some(&root_token),
        json!({"action": "unban", "minutes": null}),
    );
    let relogin = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"zhangsan","kind":"password","input":"NewPass123a"}),
    );
    let ra2_token = relogin["token"].as_str().unwrap().to_string();

    // second class, student and class admins for both sides
    let (sk_c2, pk_c2) = gen_keypair();
    let escrow2 = seal(&pk_r, &b64d(&sk_c2).unwrap()).unwrap();
    let cls2 = post(
        &a,
        &format!("{base}/api/v1/admin/classes"),
        Some(&root_token),
        json!({"name":"测试二班","grade":"高一","escrow_public_key":pk_c2,"escrow_priv_for_root":escrow2}),
    );
    let class2_id = cls2["id"].as_i64().unwrap();
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class2_id}/roster"),
        Some(&root_token),
        json!({"names":["王五"]}),
    );
    let roster2 = get(&a, &format!("{base}/api/v1/classes/{class2_id}/roster"), None);
    let wang_id = roster2
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["real_name"] == "王五")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let rc = register(&a, &base, class2_id, wang_id, "王五", "wangwu", "Pass1234c", &pk_c2, &pk_r);
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class_id}/admin"),
        Some(&root_token),
        json!({"uid": rb.uid, "class_priv_for_admin": seal(&rb.pk, &b64d(&sk_c).unwrap()).unwrap()}),
    );
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class2_id}/admin"),
        Some(&root_token),
        json!({"uid": rc.uid, "class_priv_for_admin": seal(&rc.pk, &b64d(&sk_c2).unwrap()).unwrap()}),
    );

    // transfer requires out-class then in-class approval
    let tr = post(
        &a,
        &format!("{base}/api/v1/transfers"),
        Some(&ra2_token),
        json!({"to_class_id": class2_id, "reason": "测试转班"}),
    );
    let transfer_id = tr["id"].as_i64().unwrap();
    let rb_transfers = get(&a, &format!("{base}/api/v1/admin/transfers"), Some(&rb.token));
    assert!(rb_transfers
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"].as_i64() == Some(transfer_id)));
    let step1 = post(
        &a,
        &format!("{base}/api/v1/admin/transfers/{transfer_id}/action"),
        Some(&rb.token),
        json!({"action": "approve"}),
    );
    assert_eq!(step1["status"], "out_approved");
    let step2 = post(
        &a,
        &format!("{base}/api/v1/admin/transfers/{transfer_id}/action"),
        Some(&rc.token),
        json!({"action": "approve"}),
    );
    assert_eq!(step2["status"], "approved");

    let me2 = get(&a, &format!("{base}/api/v1/me"), Some(&ra2_token));
    assert_eq!(me2["class_id"].as_i64().unwrap(), class2_id);
    let roster_after = get(&a, &format!("{base}/api/v1/classes/{class_id}/roster"), None);
    let zhang_after = roster_after
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["real_name"] == "张三")
        .unwrap();
    assert_eq!(zhang_after["claimed"], false);
    let (code, _) = call_status(
        &a,
        "GET",
        &format!("{base}/api/v1/conversations/{cg_id}/keys"),
        Some(&ra2_token),
        None,
    );
    assert_eq!(code, 403, "left class group should be inaccessible");

    // the new class group uses a rotated key that includes the transferred student
    let convs2 = get(&a, &format!("{base}/api/v1/conversations"), Some(&ra2_token));
    let cg2 = convs2
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "class_group" && c["class_id"].as_i64() == Some(class2_id))
        .unwrap();
    let cg2_id = cg2["id"].as_i64().unwrap();
    let ver2 = cg2["key_version"].as_i64().unwrap();
    let rc_convs = get(&a, &format!("{base}/api/v1/conversations"), Some(&rc.token));
    assert!(rc_convs
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"].as_i64() == Some(cg2_id)));
    let rc_keys = get(&a, &format!("{base}/api/v1/conversations/{cg2_id}/keys"), Some(&rc.token));
    let wrapped_rc = rc_keys
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key_version"].as_i64() == Some(ver2))
        .unwrap()["wrapped_key"]
        .as_str()
        .unwrap();
    let gk_rc = open_sealed(&b64e(&rc.sk), wrapped_rc).unwrap();
    let mut gk_rc_arr = [0u8; 32];
    gk_rc_arr.copy_from_slice(&gk_rc);
    let cg2_ct = b64e(&aead_encrypt(&gk_rc_arr, b"welcome transfer", cg2_id.to_string().as_bytes()));
    post(
        &a,
        &format!("{base}/api/v1/conversations/{cg2_id}/messages"),
        Some(&rc.token),
        json!({"key_version": ver2, "ciphertext": cg2_ct, "client_msg_id": uuid()}),
    );
    let ra2_keys = get(&a, &format!("{base}/api/v1/conversations/{cg2_id}/keys"), Some(&ra2_token));
    let wrapped_ra2 = ra2_keys
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key_version"].as_i64() == Some(ver2))
        .unwrap()["wrapped_key"]
        .as_str()
        .unwrap();
    let gk_ra2 = open_sealed(&b64e(&ra.sk), wrapped_ra2).unwrap();
    let mut gk_ra2_arr = [0u8; 32];
    gk_ra2_arr.copy_from_slice(&gk_ra2);
    let cg2_msgs = get(&a, &format!("{base}/api/v1/conversations/{cg2_id}/messages?after=0"), Some(&ra2_token));
    let cg2_msg = cg2_msgs
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["sender_uid"].as_i64() == Some(rc.uid))
        .unwrap();
    let cg2_pt = aead_decrypt(
        &gk_ra2_arr,
        &b64d(cg2_msg["ciphertext"].as_str().unwrap()).unwrap(),
        cg2_id.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(String::from_utf8(cg2_pt).unwrap(), "welcome transfer");

    // stats
    let stats = get(&a, &format!("{base}/api/v1/admin/stats"), Some(&root_token));
    assert!(stats["users"].as_i64().unwrap() >= 2);
    assert!(stats["messages_total"].as_i64().unwrap() >= 2);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = ra.resp;
}
