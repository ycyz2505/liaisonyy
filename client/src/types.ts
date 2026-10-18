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
  unread: number;
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
}

export interface AdminStats {
  users: number;
  online: number;
  messages_today: number;
  messages_total: number;
  classes: number;
  pending_requests: number;
  db_bytes: number;
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
  | { type: "UpdateMe"; data: { display_name: string } };

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
  | { type: "Toast"; data: string }
  | { type: "Error"; data: string }
  | { type: "Offline"; data: boolean };
