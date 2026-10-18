use liaisonyy_core::*;
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
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

fn start(exe: &str, dir: &std::path::Path, port: u16) -> Child {
    Command::new(exe)
        .args(["--data-dir", dir.to_str().unwrap(), "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_ready(base: &str, child: &mut Child) -> bool {
    let a = agent();
    for _ in 0..80 {
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        if a.get(&format!("{base}/api/v1/health")).call().is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    false
}

fn start_ready(exe: &str, dir: &std::path::Path, port: u16) -> Guard {
    let base = format!("https://127.0.0.1:{port}");
    for attempt in 0..10 {
        let mut child = start(exe, dir, port);
        if wait_ready(&base, &mut child) {
            return Guard(Some(child));
        }
        let _ = child.kill();
        let _ = child.wait();
        if attempt < 9 {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    panic!("server did not start on port {port}");
}

struct Guard(Option<Child>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(c) = &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[test]
fn crash_restart_preserves_messages() {
    let dir = std::env::temp_dir().join(format!("liaisonyy-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut port = 0u16;
    for p in 48210..48300u16 {
        if std::net::TcpListener::bind(("0.0.0.0", p)).is_ok() {
            port = p;
            break;
        }
    }
    assert!(port > 0);
    let exe = env!("CARGO_BIN_EXE_liaisonyy-server");
    let guard = start_ready(exe, &dir, port);
    let base = format!("https://127.0.0.1:{port}");
    let token_path = dir.join("setup_token.txt");
    for _ in 0..40 {
        if token_path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        token_path.exists(),
        "server did not create setup_token.txt in {} (stale server on port?)",
        dir.display()
    );
    let a = agent();
    let setup_token = std::fs::read_to_string(&token_path).unwrap().trim().to_string();

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
    let root_login = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"root","kind":"password","input":"RootPass123"}),
    );
    let root_token = root_login["token"].as_str().unwrap().to_string();

    let (sk_c, pk_c) = gen_keypair();
    let escrow = seal(&pk_r, &b64d(&sk_c).unwrap()).unwrap();
    let cls = post(
        &a,
        &format!("{base}/api/v1/admin/classes"),
        Some(&root_token),
        json!({"name":"崩溃测试班","grade":"高一","escrow_public_key":pk_c,"escrow_priv_for_root":escrow}),
    );
    let class_id = cls["id"].as_i64().unwrap();
    post(
        &a,
        &format!("{base}/api/v1/admin/classes/{class_id}/roster"),
        Some(&root_token),
        json!({"names":["测试学生"]}),
    );
    let roster = get(&a, &format!("{base}/api/v1/classes/{class_id}/roster"), None);
    let roster_id = roster.as_array().unwrap()[0]["id"].as_i64().unwrap();

    let (sk, pk) = gen_keypair();
    let (pdk, _) = gen_keypair();
    let sk_raw = b64d(&sk).unwrap();
    let pdk_raw = b64d(&pdk).unwrap();
    let salt = new_salt();
    let (kek, _) = derive_keys("Pass1234a", &salt, &params).unwrap();
    let reg = post(
        &a,
        &format!("{base}/api/v1/auth/register"),
        None,
        json!({
            "class_id": class_id,
            "roster_id": roster_id,
            "real_name": "测试学生",
            "handle": "crashstu",
            "display_name": "测试学生",
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
    let token = reg["token"].as_str().unwrap().to_string();
    let mut sk_arr = [0u8; 32];
    sk_arr.copy_from_slice(&sk_raw);

    let convs = get(&a, &format!("{base}/api/v1/conversations"), Some(&token));
    let cg = convs
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "class_group")
        .unwrap();
    let conv_id = cg["id"].as_i64().unwrap();
    let ver = cg["key_version"].as_i64().unwrap();
    let ks = get(&a, &format!("{base}/api/v1/conversations/{conv_id}/keys"), Some(&token));
    let wrapped = ks
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["key_version"].as_i64() == Some(ver))
        .unwrap()["wrapped_key"]
        .as_str()
        .unwrap();
    let gk = open_sealed(&b64e(&sk_arr), wrapped).unwrap();
    let mut gk_arr = [0u8; 32];
    gk_arr.copy_from_slice(&gk);
    for n in 0..5 {
        let ct = b64e(&aead_encrypt(
            &gk_arr,
            format!("before-crash-{n}").as_bytes(),
            conv_id.to_string().as_bytes(),
        ));
        post(
            &a,
            &format!("{base}/api/v1/conversations/{conv_id}/messages"),
            Some(&token),
            json!({"key_version": ver, "ciphertext": ct, "client_msg_id": uuid()}),
        );
    }

    // hard kill (no graceful shutdown), then restart on the same data dir
    drop(guard);
    std::thread::sleep(Duration::from_millis(500));
    let guard = start_ready(exe, &dir, port);
    let a = agent();

    let login = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"crashstu","kind":"password","input":"Pass1234a"}),
    );
    let token2 = login["token"].as_str().unwrap().to_string();
    let msgs = get(
        &a,
        &format!("{base}/api/v1/conversations/{conv_id}/messages?after=0"),
        Some(&token2),
    );
    let count = msgs
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["key_version"].as_i64() == Some(ver))
        .count();
    assert_eq!(count, 5, "messages lost after crash");

    let root2 = post(
        &a,
        &format!("{base}/api/v1/auth/login"),
        None,
        json!({"uid_or_handle":"root","kind":"password","input":"RootPass123"}),
    );
    let root_token2 = root2["token"].as_str().unwrap().to_string();
    let verify = get(&a, &format!("{base}/api/v1/admin/audit/verify"), Some(&root_token2));
    assert_eq!(verify["ok"], true, "audit chain broken after crash");

    let _ = std::fs::remove_dir_all(&dir);
}
