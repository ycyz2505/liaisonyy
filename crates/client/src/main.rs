mod net;
mod store;

use eframe::egui;
use liaisonyy_core::*;
use net::{Cmd, Ev, FriendRequestView, MessageView};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Welcome,
    Setup,
    Accounts,
    Login,
    Register,
    Main,
    Chat,
    Settings,
    Admin,
    Reset,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Messages,
    Contacts,
    Me,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KbTarget {
    None,
    Login,
    RegPassword,
    RegPin,
    Chat,
    Setup,
    Reset,
}

struct App {
    cmd: Sender<Cmd>,
    ev: Receiver<Ev>,
    settings: store::Settings,
    page: Page,
    bootstrap: Option<BootstrapResponse>,
    server_input: String,
    setup_token: String,
    setup_handle: String,
    setup_display: String,
    setup_password: String,
    recovery_code: String,
    class_id: Option<i64>,
    class_name: String,
    roster: Vec<RosterEntry>,
    selected_roster: Option<i64>,
    reg_handle: String,
    reg_display: String,
    reg_password: String,
    reg_pin: String,
    reg_policy: String,
    accounts: Vec<AccountPublic>,
    accounts_loaded: bool,
    login_account: Option<AccountPublic>,
    login_key: String,
    login_kind: String,
    login_input: String,
    user: Option<LoginResponse>,
    offline: bool,
    conversations: Vec<ConversationInfo>,
    messages: HashMap<i64, Vec<MessageView>>,
    current_conv: Option<i64>,
    chat_input: String,
    requests: Vec<FriendRequestView>,
    friends: Vec<FriendDto>,
    friend_msg: String,
    tab: Tab,
    toast: String,
    toast_at: Instant,
    error: String,
    admin_stats: Option<AdminStats>,
    admin_audit: Vec<AuditEntry>,
    admin_classes: Vec<ClassInfo>,
    admin_class_name: String,
    admin_class_grade: String,
    admin_selected_class: Option<i64>,
    admin_roster_text: String,
    admin_uid: String,
    admin_minutes: String,
    reset_code: String,
    reset_key: String,
    reset_new_password: String,
    reset_new_pin: String,
    keyboard: bool,
    kb_target: KbTarget,
    window_placed: bool,
    allow_close: bool,
    tray: Option<tray_icon::TrayIcon>,
    tray_show_id: Option<muda::MenuId>,
    tray_quit_id: Option<muda::MenuId>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        let bytes = std::fs::read("C:\\Windows\\Fonts\\simhei.ttf")
            .or_else(|_| std::fs::read("C:\\Windows\\Fonts\\msyh.ttc"))
            .unwrap_or_default();
        if !bytes.is_empty() {
            fonts.font_data.insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                f.insert(0, "cjk".to_owned());
            }
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                f.push("cjk".to_owned());
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = egui::Color32::from_rgb(244, 247, 248);
        visuals.window_fill = egui::Color32::WHITE;
        visuals.extreme_bg_color = egui::Color32::from_rgb(248, 250, 252);
        cc.egui_ctx.set_visuals(visuals);

        let settings = store::load_settings();
        let cache = store::load_cache();
        let (cmd, ev) = net::spawn(settings.clone(), cache);
        let _ = cmd.send(Cmd::Bootstrap);

        let (tray, tray_show_id, tray_quit_id) = match make_tray() {
            Ok((t, s, q)) => (Some(t), Some(s), Some(q)),
            Err(_) => (None, None, None),
        };

        let page = if settings.class_id.is_some() { Page::Accounts } else { Page::Welcome };
        Self {
            cmd,
            ev,
            server_input: settings.server.clone(),
            class_id: settings.class_id,
            class_name: settings.class_name.clone(),
            settings,
            page,
            bootstrap: None,
            setup_token: String::new(),
            setup_handle: String::new(),
            setup_display: String::new(),
            setup_password: String::new(),
            recovery_code: String::new(),
            roster: Vec::new(),
            selected_roster: None,
            reg_handle: String::new(),
            reg_display: String::new(),
            reg_password: String::new(),
            reg_pin: String::new(),
            reg_policy: "pin_allowed".into(),
            accounts: Vec::new(),
            accounts_loaded: false,
            login_account: None,
            login_key: String::new(),
            login_kind: "password".into(),
            login_input: String::new(),
            user: None,
            offline: false,
            conversations: Vec::new(),
            messages: HashMap::new(),
            current_conv: None,
            chat_input: String::new(),
            requests: Vec::new(),
            friends: Vec::new(),
            friend_msg: String::new(),
            tab: Tab::Messages,
            toast: String::new(),
            toast_at: Instant::now() - Duration::from_secs(10),
            error: String::new(),
            admin_stats: None,
            admin_audit: Vec::new(),
            admin_classes: Vec::new(),
            admin_class_name: String::new(),
            admin_class_grade: String::new(),
            admin_selected_class: None,
            admin_roster_text: String::new(),
            admin_uid: String::new(),
            admin_minutes: "60".into(),
            reset_code: String::new(),
            reset_key: String::new(),
            reset_new_password: String::new(),
            reset_new_pin: String::new(),
            keyboard: false,
            kb_target: KbTarget::None,
            window_placed: false,
            allow_close: false,
            tray,
            tray_show_id,
            tray_quit_id,
        }
    }

    fn toast(&mut self, s: impl Into<String>) {
        self.toast = s.into();
        self.toast_at = Instant::now();
    }

    fn close_or_hide(&mut self, ctx: &egui::Context) {
        let _ = self.cmd.send(Cmd::Logout);
        self.user = None;
        self.conversations.clear();
        self.messages.clear();
        self.current_conv = None;
        self.login_input.clear();
        self.page = Page::Accounts;
        self.accounts_loaded = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn logout(&mut self) {
        let _ = self.cmd.send(Cmd::Logout);
        self.user = None;
        self.conversations.clear();
        self.messages.clear();
        self.current_conv = None;
        self.page = Page::Accounts;
        self.accounts_loaded = false;
    }

    fn handle_events(&mut self) {
        while let Ok(ev) = self.ev.try_recv() {
            match ev {
                Ev::Bootstrap(b) => {
                    self.bootstrap = Some(b.clone());
                    if b.setup_required && self.page == Page::Welcome {
                        self.page = Page::Setup;
                    } else if !b.setup_required && self.page == Page::Setup {
                        self.page = Page::Welcome;
                    }
                }
                Ev::Roster(r) => self.roster = r,
                Ev::Accounts(a) => {
                    self.accounts = a;
                    self.accounts_loaded = true;
                }
                Ev::SetupDone { recovery_code } => {
                    self.recovery_code = recovery_code;
                    self.toast("初始化完成。请保存恢复码，然后用总管理员账号登录。");
                    self.login_key = self.setup_handle.clone();
                    self.login_kind = "password".into();
                    self.page = Page::Login;
                }
                Ev::LoggedIn { user, offline } => {
                    self.settings.last_uid = Some(user.uid);
                    if user.class_id.is_some() {
                        self.settings.class_id = user.class_id;
                        self.class_id = user.class_id;
                        if let Some(c) = user.classes.iter().find(|c| Some(c.id) == user.class_id) {
                            self.settings.class_name = c.name.clone();
                            self.class_name = c.name.clone();
                        }
                    }
                    store::save_settings(&self.settings);
                    self.user = Some(user);
                    self.offline = offline;
                    self.page = Page::Main;
                    self.tab = Tab::Messages;
                    self.accounts_loaded = false;
                    if let Some(cid) = self.user.as_ref().and_then(|u| u.class_id) {
                        let _ = self.cmd.send(Cmd::Accounts { class_id: cid });
                    }
                    let _ = self.cmd.send(Cmd::Requests);
                    let _ = self.cmd.send(Cmd::Friends);
                }
                Ev::LoggedOut => {
                    self.user = None;
                    self.page = Page::Accounts;
                    self.accounts_loaded = false;
                }
                Ev::Conversations(c) => {
                    self.conversations = c;
                }
                Ev::Messages { conv_id, messages, replace } => {
                    let entry = self.messages.entry(conv_id).or_default();
                    if replace {
                        *entry = messages;
                    } else {
                        for m in messages {
                            if !entry.iter().any(|x| x.id == m.id || (!m.client_msg_id.is_empty() && x.client_msg_id == m.client_msg_id)) {
                                entry.push(m);
                            }
                        }
                        entry.sort_by_key(|m| m.id);
                    }
                }
                Ev::MessageSent { conv_id, client_msg_id, id } => {
                    if let Some(list) = self.messages.get_mut(&conv_id) {
                        if let Some(m) = list.iter_mut().find(|x| x.client_msg_id == client_msg_id) {
                            m.pending = false;
                            if id > 0 {
                                m.id = id;
                            }
                        }
                    }
                }
                Ev::Requests(r) => self.requests = r,
                Ev::Friends(f) => self.friends = f,
                Ev::AdminStats(s) => self.admin_stats = Some(s),
                Ev::AdminAudit(a) => self.admin_audit = a,
                Ev::AdminClasses(c) => self.admin_classes = c,
                Ev::ResetKey { code, expires_at } => {
                    self.reset_code = code.clone();
                    let mins = ((expires_at - now()).max(0) / 60) as i64;
                    self.toast(format!("重置密钥：{code}（约 {mins} 分钟有效，只显示这一次）"));
                }
                Ev::ResetDone => {
                    self.toast("密码已重置，请用新密码登录");
                    self.page = Page::Login;
                }
                Ev::Toast(t) => self.toast(t),
                Ev::Error(e) => self.error = e,
                Ev::Offline(o) => {
                    self.offline = o;
                    if o {
                        self.toast("已进入离线模式，消息会先存在本机");
                    }
                }
            }
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        let mut toggle = false;
        let mut quit = false;
        while let Ok(e) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. } = e {
                toggle = true;
            }
        }
        while let Ok(e) = muda::MenuEvent::receiver().try_recv() {
            if Some(&e.id) == self.tray_show_id.as_ref() {
                toggle = true;
            } else if Some(&e.id) == self.tray_quit_id.as_ref() {
                quit = true;
            }
        }
        if toggle {
            self.window_placed = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if quit {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top").exact_height(52.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("liaisonyy").size(18.0).strong().color(egui::Color32::from_rgb(15, 118, 110)));
                if self.offline {
                    ui.colored_label(egui::Color32::from_rgb(180, 120, 0), "离线");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn = egui::Button::new(egui::RichText::new("关闭").size(18.0).color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(185, 28, 28));
                    if ui.add_sized([110.0, 38.0], btn).clicked() {
                        self.close_or_hide(ctx);
                    }
                });
            });
        });
    }

    fn bottom_status(&mut self, ctx: &egui::Context) {
        if !self.toast.is_empty() && self.toast_at.elapsed() < Duration::from_secs(12) {
            egui::TopBottomPanel::bottom("toast").show(ctx, |ui| {
                ui.colored_label(egui::Color32::from_rgb(15, 118, 110), &self.toast);
            });
        }
        if !self.error.is_empty() {
            egui::TopBottomPanel::bottom("err").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(185, 28, 28), &self.error);
                    if ui.small_button("×").clicked() {
                        self.error.clear();
                    }
                });
            });
        }
    }

    fn welcome_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("欢迎使用 liaisonyy");
        ui.label("杨村一中内网聊天系统");
        ui.add_space(12.0);
        ui.label("服务端地址");
        ui.text_edit_singleline(&mut self.server_input);
        if ui.button("连接服务端").clicked() {
            let _ = self.cmd.send(Cmd::SetServer(self.server_input.clone()));
            let _ = self.cmd.send(Cmd::Bootstrap);
        }
        ui.add_space(10.0);
        if let Some(b) = &self.bootstrap {
            ui.label(format!("{} · {} · v{}", b.school_name, b.server_name, b.version));
            if b.setup_required {
                ui.colored_label(egui::Color32::from_rgb(180, 120, 0), "服务端尚未初始化");
                if ui.button("初始化服务端").clicked() {
                    self.page = Page::Setup;
                }
            } else {
                ui.label("选择班级");
                let classes = b.classes.clone();
                for c in classes {
                    if ui.add_sized([ui.available_width(), 36.0], egui::Button::new(format!("{}  {}", c.name, c.grade))).clicked() {
                        self.class_id = Some(c.id);
                        self.class_name = c.name.clone();
                        self.settings.class_id = Some(c.id);
                        self.settings.class_name = c.name.clone();
                        store::save_settings(&self.settings);
                        self.page = Page::Accounts;
                        self.accounts_loaded = false;
                        let _ = self.cmd.send(Cmd::Accounts { class_id: c.id });
                    }
                }
            }
        }
        ui.add_space(12.0);
        if ui.button("uid/YY号登录（管理员）").clicked() {
            self.page = Page::Login;
            self.login_account = None;
            self.login_key.clear();
            self.login_kind = "password".into();
            self.login_input.clear();
        }
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("初始化服务端");
        ui.label("初始化口令由服务端启动时打印（SETUP TOKEN）");
        ui.text_edit_singleline(&mut self.setup_token);
        ui.label("总管理员 YY 号");
        ui.text_edit_singleline(&mut self.setup_handle);
        ui.label("显示名称");
        ui.text_edit_singleline(&mut self.setup_display);
        ui.label("密码（8 位以上，含字母和数字）");
        ui.add(egui::TextEdit::singleline(&mut self.setup_password).password(true));
        if !self.recovery_code.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(185, 28, 28), "恢复码（只显示一次，请抄下来）：");
            ui.label(egui::RichText::new(&self.recovery_code).monospace().size(16.0));
        }
        if ui.button("初始化").clicked() {
            self.error.clear();
            let _ = self.cmd.send(Cmd::Setup {
                token: self.setup_token.clone(),
                handle: self.setup_handle.clone(),
                display_name: self.setup_display.clone(),
                password: self.setup_password.clone(),
            });
        }
        if ui.button("返回").clicked() {
            self.page = Page::Welcome;
        }
    }

    fn accounts_ui(&mut self, ui: &mut egui::Ui) {
        if !self.accounts_loaded {
            if let Some(cid) = self.class_id {
                let _ = self.cmd.send(Cmd::Accounts { class_id: cid });
                self.accounts_loaded = true;
            }
        }
        ui.horizontal(|ui| {
            ui.heading(if self.class_name.is_empty() { "本班账号".to_string() } else { self.class_name.clone() });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("刷新").clicked() {
                    if let Some(cid) = self.class_id {
                        let _ = self.cmd.send(Cmd::Accounts { class_id: cid });
                    }
                }
            });
        });
        ui.add_space(6.0);
        let mut accounts = self.accounts.clone();
        accounts.sort_by(|a, b| b.unread.cmp(&a.unread).then(a.uid.cmp(&b.uid)));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for a in accounts {
                let text = format!("{}  @{}  UID {}", a.display_name, a.handle, a.uid);
                let row = egui::Button::new(egui::RichText::new(text).size(14.0)).min_size(egui::vec2(ui.available_width(), 40.0));
                let resp = ui.add(row);
                if a.unread > 0 {
                    let pos = resp.rect.right_center() + egui::vec2(-18.0, 0.0);
                    ui.painter().circle_filled(pos, 7.0, egui::Color32::BLACK);
                    if a.unread > 1 {
                        ui.painter().text(pos, egui::Align2::CENTER_CENTER, format!("{}", a.unread.min(99)), egui::FontId::proportional(9.0), egui::Color32::WHITE);
                    }
                }
                if resp.clicked() {
                    self.login_account = Some(a.clone());
                    self.login_key = a.uid.to_string();
                    self.login_kind = if a.login_policy == "pin_allowed" { "pin".into() } else { "password".into() };
                    self.login_input.clear();
                    self.page = Page::Login;
                }
            }
            if self.accounts.is_empty() {
                ui.label("还没有账号。点下方「注册新账号」。");
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_sized([120.0, 34.0], egui::Button::new("注册新账号")).clicked() {
                self.page = Page::Register;
                self.roster.clear();
                self.selected_roster = None;
                if let Some(cid) = self.class_id {
                    let _ = self.cmd.send(Cmd::Roster { class_id: cid });
                }
            }
            if ui.add_sized([120.0, 34.0], egui::Button::new("uid/YY号登录")).clicked() {
                self.login_account = None;
                self.login_key.clear();
                self.login_kind = "password".into();
                self.login_input.clear();
                self.page = Page::Login;
            }
            if ui.add_sized([90.0, 34.0], egui::Button::new("设置")).clicked() {
                self.page = Page::Settings;
            }
        });
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("登录");
        if let Some(a) = &self.login_account {
            ui.label(format!("{}  UID {}  @{}", a.display_name, a.uid, a.handle));
        } else {
            ui.label("UID 或 YY 号");
            ui.text_edit_singleline(&mut self.login_key);
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.selectable_label(self.login_kind == "pin", "PIN").clicked() {
                self.login_kind = "pin".into();
            }
            if ui.selectable_label(self.login_kind == "password", "密码").clicked() {
                self.login_kind = "password".into();
            }
        });
        ui.add(egui::TextEdit::singleline(&mut self.login_input).password(true).hint_text("可多输入几位，包含正确内容即可"));
        ui.add_space(6.0);
        if self.login_kind == "pin" {
            let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "清除", "0", "⌫"];
            egui::Grid::new("pinpad").num_columns(3).spacing([8.0, 8.0]).show(ui, |ui| {
                for (i, k) in keys.iter().enumerate() {
                    if ui.add_sized([86.0, 44.0], egui::Button::new(egui::RichText::new(*k).size(18.0))).clicked() {
                        match *k {
                            "清除" => self.login_input.clear(),
                            "⌫" => {
                                self.login_input.pop();
                            }
                            _ => self.login_input.push_str(k),
                        }
                    }
                    if (i + 1) % 3 == 0 {
                        ui.end_row();
                    }
                }
            });
        }
        ui.add_space(8.0);
        if ui.add_sized([ui.available_width(), 38.0], egui::Button::new("登录")).clicked() {
            self.error.clear();
            let _ = self.cmd.send(Cmd::Login {
                key: self.login_key.clone(),
                kind: self.login_kind.clone(),
                input: self.login_input.clone(),
            });
        }
        if ui.button("忘记密码").clicked() {
            self.page = Page::Reset;
            self.reset_key = self.login_key.clone();
        }
        if ui.button("返回账号列表").clicked() {
            self.page = Page::Accounts;
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new("PIN 也用于加密，建议至少 6 位").small().color(egui::Color32::GRAY));
    }

    fn register_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("注册账号");
        if self.roster.is_empty() {
            ui.label("正在加载名单…");
        }
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            for r in self.roster.clone() {
                if r.claimed {
                    continue;
                }
                let label = if let Some(no) = &r.student_no {
                    format!("{}（{}）", r.real_name, no)
                } else {
                    r.real_name.clone()
                };
                if ui.selectable_label(self.selected_roster == Some(r.id), label).clicked() {
                    self.selected_roster = Some(r.id);
                }
            }
        });
        ui.add_space(6.0);
        ui.label("YY 号（3-20 位，字母数字下划线，不能纯数字）");
        ui.text_edit_singleline(&mut self.reg_handle);
        ui.label("用户名（可中文，可重复）");
        ui.text_edit_singleline(&mut self.reg_display);
        ui.label("密码（8 位以上，含字母和数字）");
        ui.add(egui::TextEdit::singleline(&mut self.reg_password).password(true));
        ui.horizontal(|ui| {
            if ui.selectable_label(self.reg_policy == "pin_allowed", "只用 PIN 登录").clicked() {
                self.reg_policy = "pin_allowed".into();
            }
            if ui.selectable_label(self.reg_policy == "password_only", "必须用密码登录").clicked() {
                self.reg_policy = "password_only".into();
            }
        });
        if self.reg_policy == "pin_allowed" {
            ui.label("PIN（4-20 位纯数字，建议 ≥6 位）");
            ui.add(egui::TextEdit::singleline(&mut self.reg_pin).password(true));
        }
        ui.add_space(8.0);
        if ui.add_sized([ui.available_width(), 38.0], egui::Button::new("注册")).clicked() {
            self.error.clear();
            let rid = match self.selected_roster {
                Some(x) => x,
                None => {
                    self.error = "请先在名单中选择自己的名字".into();
                    return;
                }
            };
            let real_name = self.roster.iter().find(|r| r.id == rid).map(|r| r.real_name.clone()).unwrap_or_default();
            let cid = match self.class_id {
                Some(x) => x,
                None => {
                    self.error = "请先选择班级".into();
                    return;
                }
            };
            let _ = self.cmd.send(Cmd::Register {
                class_id: cid,
                roster_id: rid,
                real_name,
                handle: self.reg_handle.clone(),
                display_name: self.reg_display.clone(),
                password: self.reg_password.clone(),
                login_policy: self.reg_policy.clone(),
                pin: if self.reg_policy == "pin_allowed" { Some(self.reg_pin.clone()) } else { None },
            });
        }
        if ui.button("返回").clicked() {
            self.page = Page::Accounts;
        }
    }

    fn main_ui(&mut self, ui: &mut egui::Ui) {
        let name = self.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.heading(name);
            if self.offline {
                ui.colored_label(egui::Color32::from_rgb(180, 120, 0), "离线");
            }
        });
        ui.separator();
        egui::TopBottomPanel::bottom("tabs").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.tab == Tab::Messages, "消息").clicked() {
                    self.tab = Tab::Messages;
                }
                if ui.selectable_label(self.tab == Tab::Contacts, "通讯录").clicked() {
                    self.tab = Tab::Contacts;
                    let _ = self.cmd.send(Cmd::Requests);
                    let _ = self.cmd.send(Cmd::Friends);
                }
                if ui.selectable_label(self.tab == Tab::Me, "我").clicked() {
                    self.tab = Tab::Me;
                }
            });
        });
        match self.tab {
            Tab::Messages => self.messages_tab(ui),
            Tab::Contacts => self.contacts_tab(ui),
            Tab::Me => self.me_tab(ui),
        }
    }

    fn messages_tab(&mut self, ui: &mut egui::Ui) {
        let mut convs = self.conversations.clone();
        convs.sort_by(|a, b| {
            let ga = if a.kind == "class_group" { 0 } else { 1 };
            let gb = if b.kind == "class_group" { 0 } else { 1 };
            ga.cmp(&gb).then(b.last_at.cmp(&a.last_at))
        });
        if convs.is_empty() {
            ui.label("还没有会话。去通讯录添加好友吧。");
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for c in convs {
                let mut label = c.title.clone();
                if c.kind == "class_group" {
                    label = format!("【班级群】{label}");
                }
                let row = egui::Button::new(egui::RichText::new(label).size(14.0)).min_size(egui::vec2(ui.available_width(), 38.0));
                let resp = ui.add(row);
                if c.unread > 0 {
                    let pos = resp.rect.right_center() + egui::vec2(-18.0, 0.0);
                    ui.painter().circle_filled(pos, 7.0, egui::Color32::BLACK);
                }
                if resp.clicked() {
                    self.current_conv = Some(c.id);
                    self.page = Page::Chat;
                    self.chat_input.clear();
                    let _ = self.cmd.send(Cmd::OpenConversation { conv_id: c.id });
                }
            }
        });
    }

    fn contacts_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("好友");
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for f in self.friends.clone() {
                ui.label(format!("{}  @{}  UID {}", f.display_name, f.handle, f.uid));
            }
            if self.friends.is_empty() {
                ui.label("还没有好友");
            }
        });
        ui.separator();
        ui.heading("新的朋友");
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            for r in self.requests.clone() {
                if r.status != "pending" {
                    continue;
                }
                ui.horizontal(|ui| {
                    let who = if r.incoming { &r.from_name } else { &r.to_name };
                    let dir = if r.incoming { "请求加你" } else { "等待对方同意" };
                    ui.label(format!("{} {}", who, dir));
                    if r.incoming {
                        if ui.small_button("同意").clicked() {
                            let _ = self.cmd.send(Cmd::AcceptFriend { request_id: r.id, from_uid: r.from_uid });
                        }
                        if ui.small_button("拒绝").clicked() {
                            let _ = self.cmd.send(Cmd::DeclineFriend { request_id: r.id });
                        }
                    }
                });
                if r.incoming && !r.message.is_empty() {
                    ui.label(format!("附言：{}", r.message));
                }
            }
        });
        ui.separator();
        ui.heading("本班同学");
        ui.horizontal(|ui| {
            ui.label("附言");
            ui.text_edit_singleline(&mut self.friend_msg);
        });
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            let my_uid = self.user.as_ref().map(|u| u.uid).unwrap_or(0);
            for a in self.accounts.clone() {
                if a.uid == my_uid {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(format!("{}  @{}  UID {}", a.display_name, a.handle, a.uid));
                    if ui.small_button("加好友").clicked() {
                        let _ = self.cmd.send(Cmd::AddFriend { to_uid: a.uid, message: self.friend_msg.clone() });
                    }
                });
            }
        });
    }

    fn me_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(u) = &self.user {
            ui.heading(&u.display_name);
            ui.label(format!("UID {}  @{}  {}", u.uid, u.handle, if u.role == "root" { "总管理员" } else if u.role == "class_admin" { "班级管理员" } else { "学生" }));
            ui.label(if u.login_policy == "pin_allowed" { "登录方式：PIN 或密码" } else { "登录方式：仅密码" });
        }
        ui.add_space(10.0);
        if ui.button("设置").clicked() {
            self.page = Page::Settings;
        }
        if self.user.as_ref().map(|u| u.role == "root" || u.role == "class_admin").unwrap_or(false) {
            if ui.button("管理").clicked() {
                self.page = Page::Admin;
                let _ = self.cmd.send(Cmd::AdminStats);
                if self.user.as_ref().map(|u| u.role == "root").unwrap_or(false) {
                    let _ = self.cmd.send(Cmd::AdminAudit);
                    let _ = self.cmd.send(Cmd::AdminClasses);
                }
            }
        }
        if ui.button("退出登录").clicked() {
            self.logout();
        }
    }

    fn chat_ui(&mut self, ui: &mut egui::Ui) {
        let conv_id = match self.current_conv {
            Some(c) => c,
            None => {
                self.page = Page::Main;
                return;
            }
        };
        let title = self.conversations.iter().find(|c| c.id == conv_id).map(|c| c.title.clone()).unwrap_or_else(|| "聊天".into());
        ui.horizontal(|ui| {
            if ui.button("‹ 返回").clicked() {
                self.page = Page::Main;
            }
            ui.heading(title);
        });
        ui.separator();
        let msgs = self.messages.get(&conv_id).cloned().unwrap_or_default();
        egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            for m in &msgs {
                let who = if m.mine { "我".to_string() } else { m.sender_name.clone() };
                let mut text = format!("{who}: {}", m.text);
                if m.pending {
                    text.push_str("  ⏳");
                }
                ui.label(text);
            }
            if msgs.is_empty() {
                ui.label("还没有消息");
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let resp = ui.add_sized([ui.available_width() - 70.0, 34.0], egui::TextEdit::singleline(&mut self.chat_input).hint_text("输入消息"));
            let send = ui.add_sized([60.0, 34.0], egui::Button::new("发送"));
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (send.clicked() || enter) && !self.chat_input.trim().is_empty() {
                let text = self.chat_input.trim().to_string();
                let client_msg_id = uuid();
                let view = MessageView {
                    id: 0,
                    sender_uid: self.user.as_ref().map(|u| u.uid).unwrap_or(0),
                    sender_name: self.user.as_ref().map(|u| u.display_name.clone()).unwrap_or_default(),
                    text: text.clone(),
                    created_at: now(),
                    mine: true,
                    pending: true,
                    client_msg_id: client_msg_id.clone(),
                };
                self.messages.entry(conv_id).or_default().push(view);
                self.chat_input.clear();
                let _ = self.cmd.send(Cmd::SendMessage { conv_id, text, client_msg_id });
                let last = self.messages.get(&conv_id).and_then(|v| v.iter().map(|m| m.id).max()).unwrap_or(0);
                if last > 0 {
                    let _ = self.cmd.send(Cmd::MarkRead { conv_id, last_id: last });
                }
            }
        });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.label("服务器地址");
        ui.text_edit_singleline(&mut self.server_input);
        if ui.button("保存服务器地址").clicked() {
            let _ = self.cmd.send(Cmd::SetServer(self.server_input.clone()));
        }
        ui.separator();
        ui.label("隐私与数据说明");
        ui.label("聊天记录加密存储，仅展示在客户端登录后，管理后台不能看到任何聊天记录。");
        ui.label("PIN 也用于加密；PIN 越短越不安全，建议至少 6 位、最好 8 位以上。");
        ui.label("服务器是教室电脑，可能关机、还原或断网；聊天记录可能暂时不可用。");
        ui.label("不要在聊天中发送身份证号、银行卡号、密码等敏感信息。");
        ui.separator();
        ui.label("免责声明：本系统由同学自建，非学校官方服务；不保证 100% 在线，也不保证聊天记录永不丢失。");
        ui.separator();
        if ui.button("返回").clicked() {
            self.page = if self.user.is_some() { Page::Main } else { Page::Accounts };
        }
        if ui.button("退出程序").clicked() {
            self.allow_close = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn reset_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("重置密码");
        ui.label("向班级管理员/总管理员索取 8 位一次性密钥");
        ui.label("UID 或 YY 号");
        ui.text_edit_singleline(&mut self.reset_key);
        ui.label("8 位密钥");
        ui.text_edit_singleline(&mut self.reset_code);
        ui.label("新密码（8 位以上，含字母和数字）");
        ui.add(egui::TextEdit::singleline(&mut self.reset_new_password).password(true));
        ui.label("新 PIN（可选，4-20 位数字；留空则改为仅密码登录）");
        ui.add(egui::TextEdit::singleline(&mut self.reset_new_pin).password(true));
        if ui.button("重置").clicked() {
            self.error.clear();
            let _ = self.cmd.send(Cmd::Reset {
                key: self.reset_key.clone(),
                code: self.reset_code.clone(),
                new_password: self.reset_new_password.clone(),
                new_pin: if self.reset_new_pin.trim().is_empty() { None } else { Some(self.reset_new_pin.clone()) },
            });
        }
        if ui.button("返回").clicked() {
            self.page = Page::Login;
        }
    }

    fn admin_ui(&mut self, ui: &mut egui::Ui) {
        let is_root = self.user.as_ref().map(|u| u.role == "root").unwrap_or(false);
        ui.heading("管理");
        ui.horizontal(|ui| {
            if ui.button("刷新统计").clicked() {
                let _ = self.cmd.send(Cmd::AdminStats);
            }
            if is_root {
                if ui.button("刷新日志").clicked() {
                    let _ = self.cmd.send(Cmd::AdminAudit);
                }
                if ui.button("刷新班级").clicked() {
                    let _ = self.cmd.send(Cmd::AdminClasses);
                }
            }
            if ui.button("返回").clicked() {
                self.page = Page::Main;
            }
        });
        if let Some(s) = &self.admin_stats {
            ui.label(format!("账号 {} · 在线 {} · 今日消息 {} · 消息总数 {} · 班级 {} · 待处理 {} · 数据库 {} KB",
                s.users, s.online, s.messages_today, s.messages_total, s.classes, s.pending_requests, s.db_bytes / 1024));
        }
        if is_root {
            ui.separator();
            ui.heading("创建班级");
            ui.horizontal(|ui| {
                ui.label("名称");
                ui.text_edit_singleline(&mut self.admin_class_name);
                ui.label("年级");
                ui.text_edit_singleline(&mut self.admin_class_grade);
                if ui.button("创建").clicked() {
                    let _ = self.cmd.send(Cmd::CreateClass {
                        name: self.admin_class_name.clone(),
                        grade: self.admin_class_grade.clone(),
                    });
                }
            });
            ui.heading("导入名单");
            egui::ComboBox::from_label("班级").selected_text(
                self.admin_classes.iter().find(|c| Some(c.id) == self.admin_selected_class).map(|c| c.name.clone()).unwrap_or_else(|| "选择".into()),
            ).show_ui(ui, |ui| {
                for c in self.admin_classes.clone() {
                    if ui.selectable_value(&mut self.admin_selected_class, Some(c.id), c.name).clicked() {}
                }
            });
            ui.add(egui::TextEdit::multiline(&mut self.admin_roster_text).desired_rows(4).hint_text("每行一个姓名"));
            if ui.button("导入").clicked() {
                if let Some(cid) = self.admin_selected_class {
                    let names = self.admin_roster_text.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    let _ = self.cmd.send(Cmd::ImportRoster { class_id: cid, names });
                }
            }
            ui.heading("设班级管理员");
            ui.horizontal(|ui| {
                ui.label("班级");
                let mut cid = self.admin_selected_class.unwrap_or(0);
                ui.add(egui::DragValue::new(&mut cid).speed(1.0));
                ui.label("学生 UID");
                ui.text_edit_singleline(&mut self.admin_uid);
                if ui.button("设置").clicked() {
                    let uid = self.admin_uid.trim().parse::<i64>().unwrap_or(0);
                    if cid > 0 && uid > 0 {
                        let _ = self.cmd.send(Cmd::AssignAdmin { class_id: cid, uid });
                    }
                }
            });
        }
        ui.separator();
        ui.heading("生成重置密钥");
        ui.horizontal(|ui| {
            ui.label("学生 UID");
            ui.text_edit_singleline(&mut self.admin_uid);
            ui.label("有效期（分钟）");
            ui.text_edit_singleline(&mut self.admin_minutes);
            if ui.button("生成").clicked() {
                let uid = self.admin_uid.trim().parse::<i64>().unwrap_or(0);
                let mins = self.admin_minutes.trim().parse::<i64>().unwrap_or(60);
                if uid > 0 {
                    let _ = self.cmd.send(Cmd::CreateResetKey { uid, expires_minutes: mins });
                }
            }
        });
        if !self.reset_code.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(185, 28, 28), format!("重置密钥：{}（只显示一次）", self.reset_code));
        }
        if is_root && !self.admin_audit.is_empty() {
            ui.separator();
            ui.heading("最近审计");
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for a in &self.admin_audit {
                    ui.label(format!("{}  {}  {}", a.ts, a.action, a.detail.clone().unwrap_or_default()));
                }
            });
        }
        if is_root && !self.admin_classes.is_empty() {
            ui.separator();
            ui.heading("班级");
            for c in &self.admin_classes {
                ui.label(format!("#{} {} {}（未注册 {}）", c.id, c.name, c.grade, c.unclaimed));
            }
        }
    }

    fn page_ui(&mut self, ui: &mut egui::Ui) {
        match self.page {
            Page::Welcome => self.welcome_ui(ui),
            Page::Setup => self.setup_ui(ui),
            Page::Accounts => self.accounts_ui(ui),
            Page::Login => self.login_ui(ui),
            Page::Register => self.register_ui(ui),
            Page::Main => self.main_ui(ui),
            Page::Chat => self.chat_ui(ui),
            Page::Settings => self.settings_ui(ui),
            Page::Admin => self.admin_ui(ui),
            Page::Reset => self.reset_ui(ui),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();
        self.poll_tray(ctx);

        if !self.window_placed {
            if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
                let pos = egui::pos2(mon.x - 380.0 - 16.0, mon.y - 680.0 - 16.0);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                self.window_placed = true;
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_or_hide(ctx);
            }
        }

        self.top_bar(ctx);
        self.bottom_status(ctx);
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.page_ui(ui);
            });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

fn make_tray() -> Result<(tray_icon::TrayIcon, muda::MenuId, muda::MenuId), Box<dyn std::error::Error>> {
    use muda::{Menu, MenuItem};
    let menu = Menu::new();
    let show = MenuItem::new("显示 / 隐藏", true, None);
    let quit = MenuItem::new("退出", true, None);
    menu.append_items(&[&show, &quit])?;
    let show_id = show.id().clone();
    let quit_id = quit.id().clone();
    let rgba = vec![15u8, 118, 110, 255].repeat(16 * 16);
    let icon = tray_icon::Icon::from_rgba(rgba, 16, 16)?;
    let tray = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("liaisonyy")
        .with_icon(icon)
        .build()?;
    Ok((tray, show_id, quit_id))
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([380.0, 680.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_always_on_top()
            .with_title("liaisonyy"),
        ..Default::default()
    };
    eframe::run_native(
        "liaisonyy",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
