export interface KdfParams {
  m: number;
  t: number;
  p: number;
}

export interface ClassInfo {
  id: number;
  name: string;
  grade: string;
  escrow_public_key: string | null;
  unclaimed: number;
}

export interface BootstrapResponse {
  setup_required: boolean;
  setup_token_required: boolean;
  school_name: string;
  server_name: string;
  version: string;
  protocol: number;
  server_time: number;
  root_public_key: string | null;
  kdf_params: KdfParams;
  classes: ClassInfo[];
}

export interface RosterEntry {
  id: number;
  real_name: string;
  student_no: string | null;
  claimed: boolean;
}

export interface AccountPublic {
  uid: number;
  handle: string;
  display_name: string;
  class_id: number;
  role: string;
  login_policy: string;
  public_key: string;
  avatar_file_id: number | null;
  unread: number;
}

export interface PublicProfile {
  uid: number;
  handle: string;
  display_name: string;
  real_name: string;
  class_id: number | null;
  class_name: string | null;
  role: string;
  avatar_file_id: number | null;
  background_file_id: number | null;
  signature: string;
  bio: string;
  gender: string | null;
  birthday: string | null;
  socials_json: string | null;
  is_friend: boolean;
  is_blocked: boolean;
}

export interface MyProfileResponse {
  uid: number;
  handle: string;
  display_name: string;
  real_name: string;
  class_id: number | null;
  role: string;
  login_policy: string;
  avatar_file_id: number | null;
  background_file_id: number | null;
  signature: string;
  bio: string;
  gender: string | null;
  birthday: string | null;
  socials_json: string | null;
  profile_enc: string;
}

export interface LoginResponse {
  token: string;
  uid: number;
  handle: string;
  display_name: string;
  real_name: string;
  class_id: number | null;
  role: string;
  login_policy: string;
  public_key: string;
  avatar_file_id: number | null;
  server_time: number;
  matched_offset: number;
  matched_len: number;
  pwd_salt: string;
  pwd_params: KdfParams;
  pin_salt: string | null;
  pin_params: KdfParams | null;
  own_priv: string;
  pin_own: string | null;
  user_priv_adm: string;
  user_priv_class: string;
  profile_own: string;
  profile_pin: string | null;
  profile_adm: string;
  profile_class: string;
  profile_enc: string;
  classes: ClassInfo[];
}

export interface ConversationInfo {
  id: number;
  kind: string;
  title: string;
  class_id: number | null;
  key_version: number;
  member_wrap: string;
  unread: number;
  last_at: number;
  last_preview: string;
}

export interface MessageView {
  id: number;
  sender_uid: number;
  sender_name: string;
  text: string;
  created_at: number;
  mine: boolean;
  pending: boolean;
  client_msg_id: string;
}

export interface FriendRequestView {
  id: number;
  from_uid: number;
  from_name: string;
  to_uid: number;
  to_name: string;
  from_public_key: string;
  message: string;
  status: string;
  created_at: number;
  incoming: boolean;
}

export interface FriendDto {
  uid: number;
  handle: string;
  display_name: string;
  public_key: string;
  avatar_file_id: number | null;
}

export interface AdminStats {
  users: number;
  online: number;
  messages_today: number;
  messages_total: number;
  classes: number;
  pending_requests: number;
  db_bytes: number;
  reports_pending: number;
  muted: number;
  banned: number;
}

export interface NotificationDto {
  id: number;
  kind: string;
  text: string;
  created_at: number;
  read: boolean;
}

export interface AnnouncementDto {
  id: number;
  scope: string;
  class_id: number | null;
  class_name: string | null;
  title: string;
  body: string;
  created_by: number;
  created_by_name: string;
  created_at: number;
}

export interface ReportDto {
  id: number;
  reporter_uid: number;
  reporter_name: string;
  target_type: string;
  target_id: number;
  target_uid: number | null;
  target_name: string | null;
  message_id: number | null;
  conv_id: number | null;
  reason: string;
  status: string;
  created_at: number;
  handled_by: number | null;
  action: string | null;
  detail: string | null;
}

export interface TransferDto {
  id: number;
  uid: number;
  display_name: string;
  from_class_id: number;
  from_class_name: string;
  to_class_id: number;
  to_class_name: string;
  reason: string;
  status: string;
  out_by: number | null;
  in_by: number | null;
  created_at: number;
}

export interface MeResponse {
  uid: number;
  handle: string;
  display_name: string;
  real_name: string;
  class_id: number | null;
  role: string;
  login_policy: string;
  public_key: string;
  avatar_file_id: number | null;
}

export interface ClassStat {
  class_id: number;
  name: string;
  users: number;
  messages_today: number;
  messages_total: number;
}

export interface AdminDashboard {
  users: number;
  online: number;
  sessions: number;
  auth_fail_today: number;
  messages_today: number;
  messages_total: number;
  classes: ClassStat[];
  db_bytes: number;
  media_bytes: number;
  reports_pending: number;
  muted: number;
  banned: number;
}

export interface AuditVerify {
  ok: boolean;
  checked: number;
  first_bad: number | null;
}

export interface BackupInfo {
  name: string;
  created_at: number;
  bytes: number;
}

export interface AdminMessageDto {
  id: number;
  sender_uid: number;
  sender_name: string;
  text: string;
  key_version: number;
  created_at: number;
}

export interface AdminDecrypted {
  conv_id: number;
  messages: AdminMessageDto[];
}

export interface DiscoveredServer {
  url: string;
  name: string;
  version: string;
}

export interface AuditEntry {
  id: number;
  ts: number;
  actor_uid: number | null;
  action: string;
  target_type: string | null;
  target_id: string | null;
  detail: string | null;
}

export interface Settings {
  server: string;
  class_id: number | null;
  class_name: string;
  last_uid: number | null;
}

export type Cmd =
  | { type: "SetServer"; data: string }
  | { type: "Bootstrap" }
  | { type: "Setup"; data: { token: string; handle: string; display_name: string; password: string } }
  | {
      type: "Register";
      data: {
        class_id: number;
        roster_id: number;
        real_name: string;
        handle: string;
        display_name: string;
        password: string;
        login_policy: string;
        pin: string | null;
      };
    }
  | { type: "Login"; data: { key: string; kind: string; input: string } }
  | { type: "Logout" }
  | { type: "Roster"; data: { class_id: number } }
  | { type: "Accounts"; data: { class_id: number } }
  | { type: "RefreshConversations" }
  | { type: "OpenConversation"; data: { conv_id: number } }
  | { type: "SendMessage"; data: { conv_id: number; text: string; client_msg_id: string } }
  | { type: "MarkRead"; data: { conv_id: number; last_id: number } }
  | { type: "Requests" }
  | { type: "AddFriend"; data: { to_uid: number; message: string } }
  | { type: "AcceptFriend"; data: { request_id: number; from_uid: number } }
  | { type: "DeclineFriend"; data: { request_id: number } }
  | { type: "Friends" }
  | { type: "AdminStats" }
  | { type: "AdminAudit" }
  | { type: "AdminClasses" }
  | { type: "CreateClass"; data: { name: string; grade: string } }
  | { type: "ImportRoster"; data: { class_id: number; names: string[] } }
  | { type: "AssignAdmin"; data: { class_id: number; uid: number } }
  | { type: "CreateResetKey"; data: { uid: number; expires_minutes: number } }
  | { type: "Reset"; data: { key: string; code: string; new_password: string; new_pin: string | null } }
  | { type: "UpdateMe"; data: { display_name: string } }
  | { type: "GetMyProfile" }
  | {
      type: "UpdateProfile";
      data: {
        display_name: string;
        signature: string;
        bio: string;
        gender: string;
        birthday: string;
        socials: string;
        public_gender: string;
        public_birthday: string;
        public_socials: string;
      };
    }
  | { type: "UploadImage"; data: { data: string; kind: string } }
  | { type: "ChangePassword"; data: { old_input: string; new_password: string } }
  | { type: "SetPin"; data: { password_input: string; pin: string } }
  | { type: "SetLoginPolicy"; data: { password_input: string; policy: string; pin: string | null } }
  | { type: "LookupUser"; data: { query: string } }
  | { type: "GetUserProfile"; data: { uid: number } }
  | { type: "DeleteFriend"; data: { uid: number } }
  | { type: "BlockUser"; data: { uid: number } }
  | { type: "UnblockUser"; data: { uid: number } }
  | { type: "ListBlocks" }
  | { type: "FetchFile"; data: { id: number } }
  | { type: "Notifications" }
  | { type: "MarkNotificationsRead" }
  | { type: "Announcements" }
  | { type: "CreateAnnouncement"; data: { scope: string; class_id: number | null; title: string; body: string } }
  | { type: "DeleteAnnouncement"; data: { id: number } }
  | { type: "Report"; data: { target_type: string; target_id: number; reason: string } }
  | { type: "AdminReports" }
  | { type: "ResolveReport"; data: { id: number; action: string; minutes: number | null } }
  | { type: "AdminUserAction"; data: { uid: number; action: string; minutes: number | null } }
  | { type: "CreateTransfer"; data: { to_class_id: number; reason: string } }
  | { type: "MyTransfers" }
  | { type: "AdminTransfers" }
  | { type: "TransferAction"; data: { id: number; action: string } }
  | { type: "AdminDashboard" }
  | { type: "AdminAuditQuery"; data: { after: number; action: string } }
  | { type: "VerifyAudit" }
  | { type: "ExportAuditCsv" }
  | { type: "CreateBackup" }
  | { type: "ListBackups" }
  | { type: "RequestRestore"; data: { name: string } }
  | { type: "AdminUserConversations"; data: { uid: number } }
  | { type: "AdminDecryptConversation"; data: { uid: number; conv_id: number } }
  | { type: "Discover" }
  | { type: "CheckUpdate" }
  | { type: "ApplyUpdate"; data: { url: string; sha256: string } }
  | {
      type: "AdminUpdatePackage";
      data: {
        data: string;
        version: string;
        min_client: string;
        signature: string;
        url: string | null;
      };
    };

export type Ev =
  | { type: "Bootstrap"; data: BootstrapResponse }
  | { type: "Roster"; data: RosterEntry[] }
  | { type: "Accounts"; data: AccountPublic[] }
  | { type: "SetupDone"; data: { recovery_code: string } }
  | { type: "LoggedIn"; data: { user: LoginResponse; offline: boolean } }
  | { type: "LoggedOut" }
  | { type: "Conversations"; data: ConversationInfo[] }
  | { type: "Messages"; data: { conv_id: number; messages: MessageView[]; replace: boolean } }
  | { type: "MessageSent"; data: { conv_id: number; client_msg_id: string; id: number } }
  | { type: "Requests"; data: FriendRequestView[] }
  | { type: "Friends"; data: FriendDto[] }
  | { type: "AdminStats"; data: AdminStats }
  | { type: "AdminAudit"; data: AuditEntry[] }
  | { type: "AdminClasses"; data: ClassInfo[] }
  | { type: "ResetKey"; data: { code: string; expires_at: number } }
  | { type: "ResetDone" }
  | { type: "MyProfile"; data: MyProfileResponse }
  | { type: "MyProfilePrivate"; data: Record<string, unknown> }
  | { type: "UserProfile"; data: PublicProfile }
  | { type: "Blocks"; data: PublicProfile[] }
  | { type: "ImageUploaded"; data: { kind: string; file_id: number } }
  | { type: "FileData"; data: { id: number; mime: string; data: string } }
  | { type: "FriendDeleted"; data: { uid: number } }
  | { type: "Blocked"; data: { uid: number } }
  | { type: "Unblocked"; data: { uid: number } }
  | { type: "PasswordChanged" }
  | { type: "PinChanged" }
  | { type: "LoginPolicyChanged"; data: { policy: string } }
  | { type: "Notifications"; data: NotificationDto[] }
  | { type: "Announcements"; data: AnnouncementDto[] }
  | { type: "Reports"; data: ReportDto[] }
  | { type: "ReportCreated" }
  | { type: "AnnouncementCreated"; data: { id: number } }
  | { type: "Transfers"; data: TransferDto[] }
  | { type: "AdminTransfers"; data: TransferDto[] }
  | { type: "MeUpdated"; data: MeResponse }
  | { type: "AdminDashboard"; data: AdminDashboard }
  | { type: "AuditVerify"; data: AuditVerify }
  | { type: "Backups"; data: BackupInfo[] }
  | { type: "BackupCreated"; data: { name: string; path: string; bytes: number } }
  | { type: "CsvSaved"; data: { path: string } }
  | { type: "AdminConversations"; data: ConversationInfo[] }
  | { type: "AdminDecrypted"; data: AdminDecrypted }
  | { type: "Discovered"; data: DiscoveredServer[] }
  | { type: "UpdateInfo"; data: { version: string; mandatory: boolean; url: string; sha256: string } }
  | { type: "UpdateNone" }
  | { type: "Toast"; data: string }
  | { type: "Error"; data: string }
  | { type: "Offline"; data: boolean };
