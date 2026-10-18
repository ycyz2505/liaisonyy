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
            let text = r.into_string().unwrap_or_default();
            panic!("HTTP {code} {url}: {text}");
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

struct Reg {
    resp: Value,
    sk: [u8; 32],
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
    Reg { resp, sk: sk_arr, pk, uid, token }
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
    let cg_pt = aead_decrypt(&gk_b_arr, &b64d(cg_msg["ciphertext"].as_str().unwrap()).unwrap(), cg_id.to_string().as_bytes()).unwrap();
    assert_eq!(String::from_utf8(cg_pt).unwrap(), "class hello");

    // stats
    let stats = get(&a, &format!("{base}/api/v1/admin/stats"), Some(&root_token));
    assert!(stats["users"].as_i64().unwrap() >= 2);
    assert!(stats["messages_total"].as_i64().unwrap() >= 2);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = ra.resp;
}
