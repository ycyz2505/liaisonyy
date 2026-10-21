import { reactive } from "vue";
import { notify, send, serverNowMs, windowVisible } from "./api";
import { setFileData } from "./files";
import type {
  AccountPublic,
  AdminDashboard,
  AdminDecrypted,
  AdminStats,
  AnnouncementDto,
  AuditEntry,
  AuditVerify,
  BackupInfo,
  BootstrapResponse,
  ClassInfo,
  ConversationInfo,
  DiscoveredServer,
  Ev,
  FriendDto,
  FriendRequestView,
  LoginResponse,
  MessageView,
  MyProfileResponse,
  NotificationDto,
  PublicProfile,
  ReportDto,
  RosterEntry,
  TransferDto,
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
  | "Reset"
  | "Profile"
  | "Security"
  | "UserProfile"
  | "Notifications"
  | "Announcements";
export type Tab = "messages" | "contacts" | "discover" | "me";

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
  myProfile: null as MyProfileResponse | null,
  userProfile: null as PublicProfile | null,
  blocks: [] as PublicProfile[],
  profileDisplayName: "",
  profileSignature: "",
  profileBio: "",
  profileGender: "",
  profileBirthday: "",
  profileSocials: "",
  profileShowGender: true,
  profileShowBirthday: true,
  profileShowSocials: true,
  lookupQuery: "",
  secOld: "",
  secNew: "",
  secNew2: "",
  secPin: "",
  secPin2: "",
  secPolicyPassword: "",
  secPolicy: "pin_allowed" as "pin_allowed" | "password_only",
  notifications: [] as NotificationDto[],
  announcements: [] as AnnouncementDto[],
  adminReports: [] as ReportDto[],
  reportMessageId: null as number | null,
  reportReason: "",
  annTitle: "",
  annBody: "",
  annScope: "class" as "global" | "class",
  annClassId: null as number | null,
  myTransfers: [] as TransferDto[],
  adminTransfers: [] as TransferDto[],
  transferToClassId: null as number | null,
  transferReason: "",
  adminDashboard: null as AdminDashboard | null,
  auditVerify: null as AuditVerify | null,
  auditFilter: "",
  backups: [] as BackupInfo[],
  adminConvs: [] as ConversationInfo[],
  adminDecrypted: null as AdminDecrypted | null,
  decryptUid: "",
  discovered: [] as DiscoveredServer[],
  discovering: false,
  updateInfo: null as { version: string; mandatory: boolean; url: string; sha256: string } | null,
  updateGate: "checking" as "checking" | "ready" | "required" | "error",
  updateError: "",
  updateApplying: false,
  updateProgress: "",
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
  const now = new Date(serverNowMs());
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

export function resetRegisterForm(): void {
  state.selectedRoster = null;
  state.regHandle = "";
  state.regDisplay = "";
  state.regPassword = "";
  state.regPin = "";
  state.regPolicy = "pin_allowed";
  state.roster = [];
}

export function clearSensitiveForms(): void {
  resetRegisterForm();
  state.loginAccount = null;
  state.loginInput = "";
  state.resetKey = "";
  state.resetCode = "";
  state.resetNewPassword = "";
  state.resetNewPin = "";
  state.setupToken = "";
  state.setupPassword = "";
  state.setupHandle = "";
  state.setupDisplay = "";
  state.chatInput = "";
  state.friendMsg = "";
  state.recoveryCode = "";
  state.adminUid = "";
  state.secOld = "";
  state.secNew = "";
  state.secNew2 = "";
  state.secPin = "";
  state.secPin2 = "";
  state.secPolicyPassword = "";
  state.lookupQuery = "";
  state.profileDisplayName = "";
  state.profileSignature = "";
  state.profileBio = "";
  state.profileGender = "";
  state.profileBirthday = "";
  state.profileSocials = "";
  state.myProfile = null;
  state.userProfile = null;
  state.blocks = [];
  state.reportMessageId = null;
  state.reportReason = "";
  state.annTitle = "";
  state.annBody = "";
  state.notifications = [];
  state.announcements = [];
  state.adminReports = [];
  state.myTransfers = [];
  state.adminTransfers = [];
  state.transferToClassId = null;
  state.transferReason = "";
  state.adminDashboard = null;
  state.auditVerify = null;
  state.auditFilter = "";
  state.backups = [];
  state.adminConvs = [];
  state.adminDecrypted = null;
  state.decryptUid = "";
}

export function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function notifyIncoming(msgs: MessageView[]): Promise<void> {
  if (!state.user) return;
  const incoming = msgs.filter(
    (m) => m.sender_uid !== state.user!.uid && m.id > 0,
  );
  if (incoming.length === 0) return;
  if (await windowVisible()) return;
  const last = incoming[incoming.length - 1];
  await notify("liaisonyy", `${last.sender_name} 发来一条消息`);
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
      showToast("初始化完成。请先抄下恢复码，再点「我已保存，去登录」。");
      state.loginKey = state.setupHandle;
      state.loginKind = "password";
      state.loginInput = "";
      state.setupPassword = "";
      state.setupToken = "";
      break;
    case "LoggedIn": {
      resetRegisterForm();
      state.loginInput = "";
      state.resetCode = "";
      state.resetNewPassword = "";
      state.resetNewPin = "";
      state.chatInput = "";
      state.friendMsg = "";
      state.recoveryCode = "";
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
      send({ type: "GetMyProfile" });
      send({ type: "ListBlocks" });
      send({ type: "Notifications" });
      send({ type: "Announcements" });
      send({ type: "MyTransfers" });
      if (ev.data.offline) showToast("已进入离线模式，消息会先存在本机");
      break;
    }
    case "LoggedOut":
      state.user = null;
      state.page = "Accounts";
      state.conversations = [];
      state.messages = {};
      state.currentConv = null;
      clearSensitiveForms();
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
      if (!replace) void notifyIncoming(messages);
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
      const mins = Math.max(0, Math.round((ev.data.expires_at - serverNowMs() / 1000) / 60));
      showToast(`重置密钥：${ev.data.code}（约 ${mins} 分钟有效，只显示这一次）`);
      break;
    }
    case "ResetDone":
      showToast("密码已重置，请用新密码登录");
      state.resetCode = "";
      state.resetNewPassword = "";
      state.resetNewPin = "";
      state.page = "Login";
      break;
    case "MyProfile": {
      state.myProfile = ev.data;
      state.profileDisplayName = ev.data.display_name;
      state.profileSignature = ev.data.signature;
      state.profileBio = ev.data.bio;
      if (state.user) {
        state.user.display_name = ev.data.display_name;
        state.user.login_policy = ev.data.login_policy;
        state.user.avatar_file_id = ev.data.avatar_file_id;
      }
      break;
    }
    case "MyProfilePrivate": {
      const p = ev.data;
      const str = (k: string) => (typeof p[k] === "string" ? (p[k] as string) : "");
      state.profileGender = str("gender");
      state.profileBirthday = str("birthday");
      state.profileSocials = str("socials");
      break;
    }
    case "UserProfile":
      state.userProfile = ev.data;
      state.page = "UserProfile";
      break;
    case "Blocks":
      state.blocks = ev.data;
      break;
    case "ImageUploaded":
      break;
    case "FileData":
      setFileData(ev.data.id, ev.data.mime, ev.data.data);
      break;
    case "FriendDeleted": {
      const { uid } = ev.data;
      if (state.userProfile?.uid === uid) state.userProfile.is_friend = false;
      state.friends = state.friends.filter((f) => f.uid !== uid);
      break;
    }
    case "Blocked": {
      const { uid } = ev.data;
      if (state.userProfile?.uid === uid) state.userProfile.is_blocked = true;
      break;
    }
    case "Unblocked": {
      const { uid } = ev.data;
      if (state.userProfile?.uid === uid) state.userProfile.is_blocked = false;
      state.blocks = state.blocks.filter((b) => b.uid !== uid);
      break;
    }
    case "PasswordChanged":
      state.secOld = "";
      state.secNew = "";
      state.secNew2 = "";
      break;
    case "PinChanged":
      state.secPin = "";
      state.secPin2 = "";
      state.secPolicyPassword = "";
      if (state.user) state.user.login_policy = "pin_allowed";
      break;
    case "LoginPolicyChanged":
      if (state.user) state.user.login_policy = ev.data.policy;
      state.secPolicyPassword = "";
      state.secPin = "";
      state.secPin2 = "";
      break;
    case "Notifications":
      state.notifications = ev.data;
      break;
    case "Announcements":
      state.announcements = ev.data;
      break;
    case "Reports":
      state.adminReports = ev.data;
      break;
    case "ReportCreated":
      state.reportMessageId = null;
      state.reportReason = "";
      break;
    case "AnnouncementCreated":
      state.annTitle = "";
      state.annBody = "";
      break;
    case "Transfers":
      state.myTransfers = ev.data;
      break;
    case "AdminTransfers":
      state.adminTransfers = ev.data;
      break;
    case "MeUpdated": {
      if (!state.user) break;
      const oldClass = state.user.class_id;
      state.user.class_id = ev.data.class_id;
      state.user.display_name = ev.data.display_name;
      state.user.login_policy = ev.data.login_policy;
      state.user.avatar_file_id = ev.data.avatar_file_id;
      if (ev.data.class_id !== oldClass) {
        state.classId = ev.data.class_id;
        const cls = state.bootstrap?.classes.find((c) => c.id === ev.data.class_id);
        state.className = cls?.name ?? "";
        if (ev.data.class_id != null) {
          send({ type: "Accounts", data: { class_id: ev.data.class_id } });
        }
        showToast("你的班级信息已更新");
      }
      break;
    }
    case "AdminDashboard":
      state.adminDashboard = ev.data;
      break;
    case "AuditVerify":
      state.auditVerify = ev.data;
      showToast(
        ev.data.ok
          ? `审计链完整（已校验 ${ev.data.checked} 条）`
          : `审计链在第 ${ev.data.first_bad} 条断裂`,
      );
      break;
    case "Backups":
      state.backups = ev.data;
      break;
    case "BackupCreated":
      break;
    case "CsvSaved":
      break;
    case "AdminConversations":
      state.adminConvs = ev.data;
      break;
    case "AdminDecrypted":
      state.adminDecrypted = ev.data;
      break;
    case "Discovered": {
      state.discovering = false;
      state.discovered = ev.data;
      if (ev.data.length === 1) {
        const s = ev.data[0];
        state.serverInput = s.url;
        send({ type: "SetServer", data: s.url });
        send({ type: "Bootstrap" });
        showToast(`已发现服务端：${s.name}`);
      } else if (ev.data.length > 1) {
        showToast("发现多个服务端，请先联系管理员确认，不要随意连接");
      } else {
        showToast("没有发现服务端，请手动输入地址");
      }
      break;
    }
    case "UpdateInfo":
      state.updateInfo = ev.data;
      state.updateGate = "required";
      state.updateError = "";
      state.updateApplying = false;
      state.updateProgress = "";
      break;
    case "UpdateNone":
      state.updateInfo = null;
      state.updateGate = "ready";
      state.updateError = "";
      state.updateApplying = false;
      state.updateProgress = "";
      break;
    case "UpdateFailed":
      state.updateGate = "error";
      state.updateError = ev.data;
      state.updateApplying = false;
      state.updateProgress = "";
      break;
    case "UpdateProgress":
      state.updateApplying = true;
      state.updateProgress = ev.data;
      break;
    case "UpdateApplyFailed":
      state.updateApplying = false;
      state.updateProgress = "";
      state.updateGate = "required";
      state.error = ev.data;
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
