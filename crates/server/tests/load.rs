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

fn post(agent: &ureq::Agent, url: &str, token: Option<&str>, body: Value) -> Value {
    call(agent, "POST", url, token, Some(body))
}

fn get(agent: &ureq::Agent, url: &str, token: Option<&str>) -> Value {
    call(agent, "GET", url, token, None)
}

struct ServerGuard(std::process::Child);
impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Student {
    uid: i64,
    token: String,
    sk: [u8; 32],
}

#[test]
fn load_23_concurrent() {
    let dir = std::env::temp_dir().join(format!("liaisonyy-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut port = 0u16;
    for p in 48110..48200u16 {
        if std::net::TcpListener::bind(("127.0.0.1", p)).is_ok() {
            port = p;
            break;
        }
    }
    assert!(port > 0);
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
    for _ in 0..120 {
        if a.get(&format!("{base}/api/v1/health")).call().is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    assert!(ready, "server did not start");
    let setup_token = std::fs::read_to_string(dir.join("setup_token.txt")).unwrap().trim().to_string();

    let (sk_r, pk_r) = gen_keypair();
    let salt = new_salt();
    let params = KdfParams::default();
    let (kek, _) = derive_keys("RootPass123", &salt, &params).unwrap();
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
            "root_own": wrap_key(&kek, &b64d(&sk_r).unwrap()),
            "root_recovery": wrap_key(&kek, &b64d(&sk_r).unwrap()),
        }),
    );
    let login = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"root","kind":"password","input":"RootPass123"}),
    );
    let root_token = login["token"].as_str().unwrap().to_string();

    let (sk_c, pk_c) = gen_keypair();
    let escrow = seal(&pk_r, &b64d(&sk_c).unwrap()).unwrap();
    let cls = post(
        &a,
        &format!("{base}/api/v1/admin/classes"),
        Some(&root_token),
        json!({"name":"压力测试班","grade":"高一","escrow_public_key":pk_c,"escrow_priv_for_root":escrow}),
    );
    let class_id = cls["id"].as_i64().unwrap();
    let names: Vec<String> = (1..=23).map(|i| format!("学生{i:02}")).collect();
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class_id}/roster"),
        Some(&root_token),
        json!({"names": names}),
    );
    let roster = get(&a, &format!("{base}/api/v1/classes/{class_id}/roster"), None);
    let entries = roster.as_array().unwrap().clone();

    let mut students: Vec<Student> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let real_name = entry["real_name"].as_str().unwrap();
        let roster_id = entry["id"].as_i64().unwrap();
        let (sk, pk) = gen_keypair();
        let (pdk, _) = gen_keypair();
        let sk_raw = b64d(&sk).unwrap();
        let pdk_raw = b64d(&pdk).unwrap();
        let salt = new_salt();
        let (kek, _) = derive_keys("Pass1234a", &salt, &params).unwrap();
        let resp = post(
            &a,
            &format!("{base}/api/v1/auth/register"),
            None,
            json!({
                "class_id": class_id,
                "roster_id": roster_id,
                "real_name": real_name,
                "handle": format!("stu{i:02}"),
                "display_name": real_name,
                "password": "Pass1234a",
                "login_policy": "password_only",
                "pin": null,
                "public_key": pk,
                "pwd_salt": salt,
                "pwd_params": params,
                "pin_salt": null,
                "pin_params": null,
                "own_priv": wrap_key(&kek, &sk_raw),
                "pin_own": null,
                "user_priv_adm": seal(&pk_r, &sk_raw).unwrap(),
                "user_priv_class": seal(&pk_c, &sk_raw).unwrap(),
                "profile_own": wrap_key(&kek, &pdk_raw),
                "profile_pin": null,
                "profile_adm": seal(&pk_r, &pdk_raw).unwrap(),
                "profile_class": seal(&pk_c, &pdk_raw).unwrap(),
            }),
        );
        let uid = resp["uid"].as_i64().unwrap();
        let token = resp["token"].as_str().unwrap().to_string();
        let mut sk_arr = [0u8; 32];
        sk_arr.copy_from_slice(&sk_raw);
        students.push(Student { uid, token, sk: sk_arr });
    }
    assert_eq!(students.len(), 23);

    // every student obtains the current class-group key
    let mut keys = Vec::new();
    for s in &students {
        let convs = get(&a, &format!("{base}/api/v1/conversations"), Some(&s.token));
        let cg = convs
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "class_group")
            .unwrap();
        let conv_id = cg["id"].as_i64().unwrap();
        let ver = cg["key_version"].as_i64().unwrap();
        let ks = get(&a, &format!("{base}/api/v1/conversations/{conv_id}/keys"), Some(&s.token));
        let wrapped = ks
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["key_version"].as_i64() == Some(ver))
            .unwrap()["wrapped_key"]
            .as_str()
            .unwrap();
        let gk = open_sealed(&b64e(&s.sk), wrapped).unwrap();
        let mut gk_arr = [0u8; 32];
        gk_arr.copy_from_slice(&gk);
        keys.push((conv_id, ver, gk_arr));
    }
    let (conv_id, ver, _) = keys[0];

    let mut handles = Vec::new();
    for (i, s) in students.iter().enumerate() {
        let token = s.token.clone();
        let base = base.clone();
        let gk = keys[i].2;
        let uid = s.uid;
        handles.push(std::thread::spawn(move || {
            let a = agent();
            for n in 0..8 {
                let text = format!("load-{uid}-{n}");
                let ct = b64e(&aead_encrypt(&gk, text.as_bytes(), conv_id.to_string().as_bytes()));
                post(
                    &a,
                    &format!("{base}/api/v1/conversations/{conv_id}/messages"),
                    Some(&token),
                    json!({"key_version": ver, "ciphertext": ct, "client_msg_id": uuid()}),
                );
            }
        }));
    }
    for h in handles {
        h.join().expect("sender thread panicked");
    }

    let msgs = get(
        &a,
        &format!("{base}/api/v1/conversations/{conv_id}/messages?after=0"),
        Some(&students[0].token),
    );
    let mut all = msgs.as_array().unwrap().clone();
    let last_id = all
        .iter()
        .filter_map(|m| m["id"].as_i64())
        .max()
        .unwrap_or(0);
    if last_id > 0 {
        let more = get(
            &a,
            &format!("{base}/api/v1/conversations/{conv_id}/messages?after={last_id}"),
            Some(&students[0].token),
        );
        all.extend(more.as_array().unwrap().clone());
    }
    let count = all
        .iter()
        .filter(|m| m["key_version"].as_i64() == Some(ver))
        .count();
    assert!(count >= 184, "expected 184 messages, got {count}");
    assert!(a.get(&format!("{base}/api/v1/health")).call().is_ok());

    let _ = std::fs::remove_dir_all(&dir);
}
