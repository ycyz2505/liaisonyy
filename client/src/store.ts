import { reactive } from "vue";
import { send } from "./api";
import type {
  AccountPublic,
  AdminStats,
  AuditEntry,
  BootstrapResponse,
  ClassInfo,
  ConversationInfo,
  Ev,
  FriendDto,
  FriendRequestView,
  LoginResponse,
  MessageView,
  RosterEntry,
} from "./types";

export type Page =
  | "Welcome"
  | "Setup"
  | "Accounts"
  | "Login"
  | "Register"
  | "Main"
  | "Chat"
  | "Settings"
  | "Admin"
  | "Reset";
export type Tab = "messages" | "contacts" | "me";

export const state = reactive({
  page: "Welcome" as Page,
  tab: "messages" as Tab,
  serverInput: "",
  bootstrap: null as BootstrapResponse | null,
  classId: null as number | null,
  className: "",
  roster: [] as RosterEntry[],
  selectedRoster: null as number | null,
  accounts: [] as AccountPublic[],
  loginAccount: null as AccountPublic | null,
  loginKey: "",
  loginKind: "password" as "password" | "pin",
  loginInput: "",
  user: null as LoginResponse | null,
  offline: false,
  conversations: [] as ConversationInfo[],
  messages: {} as Record<number, MessageView[]>,
  currentConv: null as number | null,
  chatInput: "",
  requests: [] as FriendRequestView[],
  friends: [] as FriendDto[],
  friendMsg: "",
  adminStats: null as AdminStats | null,
  adminAudit: [] as AuditEntry[],
  adminClasses: [] as ClassInfo[],
  adminClassName: "",
  adminClassGrade: "",
  adminSelectedClass: null as number | null,
  adminRosterText: "",
  adminUid: "",
  adminMinutes: "60",
  resetCode: "",
  resetKey: "",
  resetNewPassword: "",
  resetNewPin: "",
  recoveryCode: "",
  setupToken: "",
  setupHandle: "",
  setupDisplay: "",
  setupPassword: "",
  regHandle: "",
  regDisplay: "",
  regPassword: "",
  regPin: "",
  regPolicy: "pin_allowed" as "pin_allowed" | "password_only",
  toast: "",
  error: "",
});

let toastTimer: number | undefined;

export function showToast(msg: string): void {
  state.toast = msg;
  if (toastTimer) window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    if (state.toast === msg) state.toast = "";
  }, 12000);
}

export function showError(msg: string): void {
  state.error = msg;
}

export function formatTime(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const now = new Date();
  if (d.toDateString() === now.toDateString()) {
    return `${d.getHours().toString().padStart(2, "0")}:${d
      .getMinutes()
      .toString()
      .padStart(2, "0")}`;
  }
  return `${d.getMonth() + 1}/${d.getDate()} ${d
    .getHours()
    .toString()
    .padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}

export function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function handleEv(ev: Ev): void {
  switch (ev.type) {
    case "Bootstrap": {
      state.bootstrap = ev.data;
      if (ev.data.setup_required && state.page === "Welcome") state.page = "Setup";
      if (!ev.data.setup_required && state.page === "Setup") state.page = "Welcome";
      break;
    }
    case "Roster":
      state.roster = ev.data;
      break;
    case "Accounts":
      state.accounts = ev.data;
      break;
    case "SetupDone":
      state.recoveryCode = ev.data.recovery_code;
      showToast("初始化完成。请保存恢复码，然后用总管理员账号登录。");
      state.loginKey = state.setupHandle;
      state.loginKind = "password";
      state.loginInput = "";
      state.page = "Login";
      break;
    case "LoggedIn": {
      state.user = ev.data.user;
      state.offline = ev.data.offline;
      if (ev.data.user.class_id != null) {
        state.classId = ev.data.user.class_id;
        const cls = ev.data.user.classes.find((c) => c.id === ev.data.user.class_id);
        if (cls) state.className = cls.name;
        send({ type: "Accounts", data: { class_id: ev.data.user.class_id } });
      }
      state.page = "Main";
      state.tab = "messages";
      send({ type: "Requests" });
      send({ type: "Friends" });
      if (ev.data.offline) showToast("已进入离线模式，消息会先存在本机");
      break;
    }
    case "LoggedOut":
      state.user = null;
      state.page = "Accounts";
      state.conversations = [];
      state.messages = {};
      state.currentConv = null;
      if (state.classId != null) send({ type: "Accounts", data: { class_id: state.classId } });
      break;
    case "Conversations":
      state.conversations = ev.data;
      break;
    case "Messages": {
      const { conv_id, messages, replace } = ev.data;
      if (replace) {
        state.messages[conv_id] = messages;
      } else {
        const merged = [...(state.messages[conv_id] ?? [])];
        for (const m of messages) {
          if (
            !merged.some(
              (x) =>
                x.id === m.id ||
                (m.client_msg_id !== "" && x.client_msg_id === m.client_msg_id),
            )
          ) {
            merged.push(m);
          }
        }
        merged.sort((a, b) => a.id - b.id);
        state.messages[conv_id] = merged;
      }
      break;
    }
    case "MessageSent": {
      const { conv_id, client_msg_id, id } = ev.data;
      const list = state.messages[conv_id];
      if (list) {
        const m = list.find((x) => x.client_msg_id === client_msg_id);
        if (m) {
          m.pending = false;
          if (id > 0) m.id = id;
        }
      }
      break;
    }
    case "Requests":
      state.requests = ev.data;
      break;
    case "Friends":
      state.friends = ev.data;
      break;
    case "AdminStats":
      state.adminStats = ev.data;
      break;
    case "AdminAudit":
      state.adminAudit = ev.data;
      break;
    case "AdminClasses":
      state.adminClasses = ev.data;
      break;
    case "ResetKey": {
      state.resetCode = ev.data.code;
      const mins = Math.max(0, Math.round((ev.data.expires_at - Date.now() / 1000) / 60));
      showToast(`重置密钥：${ev.data.code}（约 ${mins} 分钟有效，只显示这一次）`);
      break;
    }
    case "ResetDone":
      showToast("密码已重置，请用新密码登录");
      state.page = "Login";
      break;
    case "Toast":
      showToast(ev.data);
      break;
    case "Error":
      showError(ev.data);
      break;
    case "Offline":
      state.offline = ev.data;
      if (ev.data) showToast("已进入离线模式，消息会先存在本机");
      break;
  }
}
