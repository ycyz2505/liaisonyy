<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { send } from "../api";
import { formatTime, showError, state } from "../store";
import type { ReportDto } from "../types";

const updVersion = ref("");
const updMin = ref("");
const updSig = ref("");
const updData = ref("");
const updUrl = ref("");

function pickUpdate(e: Event) {
  const input = e.target as HTMLInputElement;
  const f = input.files?.[0];
  if (!f) return;
  if (f.size > 20 * 1024 * 1024) {
    showError("更新包最大 20 MB");
    return;
  }
  const reader = new FileReader();
  reader.onload = () => {
    updData.value = String(reader.result ?? "");
  };
  reader.readAsDataURL(f);
  input.value = "";
}

function uploadUpdate() {
  if (!updVersion.value || !updMin.value || !updSig.value || !updData.value) {
    showError("请填写版本号、最低版本、签名并选择更新包");
    return;
  }
  send({
    type: "AdminUpdatePackage",
    data: {
      data: updData.value,
      version: updVersion.value.trim(),
      min_client: updMin.value.trim(),
      signature: updSig.value.trim(),
      url: updUrl.value.trim() || null,
    },
  });
}

const isRoot = computed(() => state.user?.role === "root");

function refresh() {
  send({ type: "AdminStats" });
  send({ type: "AdminReports" });
  send({ type: "AdminTransfers" });
  send({ type: "AdminDashboard" });
  if (isRoot.value) {
    send({ type: "AdminAuditQuery", data: { after: 0, action: state.auditFilter.trim() } });
    send({ type: "AdminClasses" });
    send({ type: "ListBackups" });
  }
}

onMounted(refresh);

function queryAudit() {
  send({ type: "AdminAuditQuery", data: { after: 0, action: state.auditFilter.trim() } });
}

function verifyAudit() {
  send({ type: "VerifyAudit" });
}

function exportAudit() {
  send({ type: "ExportAuditCsv" });
}

function createBackup() {
  send({ type: "CreateBackup" });
}

function loadBackups() {
  send({ type: "ListBackups" });
}

function restore(name: string) {
  if (!window.confirm(`确定在服务端下次启动时恢复备份 ${name} 吗？当前数据会先移到 pre-restore 目录。`)) return;
  send({ type: "RequestRestore", data: { name } });
}

function loadUserConvs() {
  const uid = Number(state.decryptUid.trim());
  if (!uid) return;
  send({ type: "AdminUserConversations", data: { uid } });
}

function decryptConv(convId: number) {
  const uid = Number(state.decryptUid.trim());
  if (!uid) return;
  send({ type: "AdminDecryptConversation", data: { uid, conv_id: convId } });
}

function transferAct(id: number, action: string) {
  send({ type: "TransferAction", data: { id, action } });
}

function isPending(s: string): boolean {
  return s === "pending" || s === "out_approved" || s === "in_approved";
}

function statusText(s: string): string {
  return (
    {
      pending: "待审批",
      out_approved: "转出班已同意",
      in_approved: "转入班已同意",
      approved: "已通过",
      rejected: "已拒绝",
    }[s] ?? s
  );
}

function resolveReport(r: ReportDto, action: string) {
  const minutes = action === "mute" ? 60 : null;
  send({ type: "ResolveReport", data: { id: r.id, action, minutes } });
}

function userAction(action: string) {
  const uid = Number(state.adminUid.trim());
  if (!uid) return;
  const minutes = action === "mute" ? Number(state.adminMinutes.trim()) || 60 : null;
  send({ type: "AdminUserAction", data: { uid, action, minutes } });
}

function createClass() {
  send({
    type: "CreateClass",
    data: { name: state.adminClassName.trim(), grade: state.adminClassGrade.trim() },
  });
}

function importRoster() {
  if (state.adminSelectedClass == null) return;
  const names = state.adminRosterText
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
  send({ type: "ImportRoster", data: { class_id: state.adminSelectedClass, names } });
}

function assignAdmin() {
  const uid = Number(state.adminUid.trim());
  if (state.adminSelectedClass == null || !uid) return;
  send({ type: "AssignAdmin", data: { class_id: state.adminSelectedClass, uid } });
}

function createResetKey() {
  const uid = Number(state.adminUid.trim());
  const mins = Number(state.adminMinutes.trim()) || 60;
  if (!uid) return;
  send({ type: "CreateResetKey", data: { uid, expires_minutes: mins } });
}
</script>

<template>
  <div class="page">
    <div class="row" style="align-items: baseline">
      <h2 style="flex: 1">管理</h2>
    </div>
    <div class="row">
      <button @click="refresh">刷新</button>
    </div>

    <div v-if="state.adminStats" class="stats-grid" style="margin-top: 10px">
      <div class="stat"><b>{{ state.adminStats.users }}</b><span>账号</span></div>
      <div class="stat"><b>{{ state.adminStats.online }}</b><span>在线</span></div>
      <div class="stat"><b>{{ state.adminStats.messages_today }}</b><span>今日消息</span></div>
      <div class="stat"><b>{{ state.adminStats.messages_total }}</b><span>消息总数</span></div>
      <div class="stat"><b>{{ state.adminStats.classes }}</b><span>班级</span></div>
      <div class="stat"><b>{{ state.adminStats.pending_requests }}</b><span>待处理申请</span></div>
      <div class="stat">
        <b>{{ (state.adminStats.db_bytes / 1024).toFixed(0) }}</b><span>数据库 KB</span>
      </div>
      <div class="stat"><b>{{ state.adminStats.reports_pending }}</b><span>待处理举报</span></div>
      <div class="stat"><b>{{ state.adminStats.muted }}</b><span>禁言中</span></div>
      <div class="stat"><b>{{ state.adminStats.banned }}</b><span>封禁</span></div>
    </div>

    <div v-if="state.adminDashboard" class="card">
      <div class="stats-grid">
        <div class="stat"><b>{{ state.adminDashboard.sessions }}</b><span>有效会话</span></div>
        <div class="stat"><b>{{ state.adminDashboard.auth_fail_today }}</b><span>24h 登录失败</span></div>
        <div class="stat">
          <b>{{ (state.adminDashboard.media_bytes / 1024).toFixed(0) }}</b><span>媒体 KB</span>
        </div>
        <div class="stat"><b>{{ state.adminDashboard.online }}</b><span>在线账号</span></div>
      </div>
      <h3>班级概览</h3>
      <div
        v-for="c in state.adminDashboard.classes"
        :key="c.class_id"
        class="small"
        style="padding: 2px 0"
      >
        {{ c.name }} · {{ c.users }} 人 · 今日 {{ c.messages_today }} 条 · 累计
        {{ c.messages_total }} 条
      </div>
    </div>

    <div class="sep"></div>
    <div class="row" style="align-items: baseline">
      <h3 style="flex: 1; margin: 0">举报处理</h3>
      <button class="link fixed" @click="send({ type: 'AdminReports' })">刷新</button>
    </div>
    <div v-if="state.adminReports.length === 0" class="empty">没有举报</div>
    <div v-for="r in state.adminReports" :key="r.id" class="card">
      <div class="row">
        <div class="grow">
          <b>{{ r.target_name || ("UID " + r.target_id) }}</b>
          <span class="muted small">
            · {{ r.target_type === "message" ? "消息" : "用户" }} · 举报人 {{ r.reporter_name }}
          </span>
        </div>
        <span class="badge" :class="r.status === 'pending' ? 'badge-warn' : 'badge-ok'">
          {{ r.status === "pending" ? "待处理" : "已处理" }}
        </span>
      </div>
      <div class="small" style="margin-top: 4px">{{ r.reason }}</div>
      <div class="small muted">{{ formatTime(r.created_at) }}</div>
      <div v-if="r.status === 'pending'" class="row" style="margin-top: 6px; flex-wrap: wrap">
        <button class="fixed" @click="resolveReport(r, 'dismiss')">忽略</button>
        <button v-if="r.message_id" class="fixed" @click="resolveReport(r, 'hide_message')">
          隐藏消息
        </button>
        <button v-if="r.target_uid" class="fixed" @click="resolveReport(r, 'mute')">禁言 60 分钟</button>
        <button v-if="r.target_uid" class="fixed danger" @click="resolveReport(r, 'ban')">封禁</button>
      </div>
      <div v-else class="small muted">处理结果：{{ r.detail }}</div>
    </div>

    <div class="sep"></div>
    <h3>手动处理用户</h3>
    <div class="row">
      <input v-model="state.adminUid" placeholder="用户 UID" />
      <input v-model="state.adminMinutes" placeholder="禁言分钟" />
    </div>
    <div class="row" style="margin-top: 8px; flex-wrap: wrap">
      <button class="fixed" @click="userAction('mute')">禁言</button>
      <button class="fixed" @click="userAction('unmute')">解除禁言</button>
      <button class="fixed danger" @click="userAction('ban')">封禁</button>
      <button class="fixed" @click="userAction('unban')">解封</button>
    </div>

    <button style="margin-top: 10px" @click="state.page = 'Announcements'; send({ type: 'Announcements' })">
      公告管理
    </button>

    <div class="sep"></div>
    <div class="row" style="align-items: baseline">
      <h3 style="flex: 1; margin: 0">转班审批</h3>
      <button class="link fixed" @click="send({ type: 'AdminTransfers' })">刷新</button>
    </div>
    <div v-if="state.adminTransfers.length === 0" class="empty">没有转班申请</div>
    <div v-for="t in state.adminTransfers" :key="t.id" class="card">
      <div class="row" style="align-items: baseline">
        <b class="grow">{{ t.display_name }}</b>
        <span class="badge" :class="isPending(t.status) ? 'badge-warn' : 'badge-ok'">
          {{ statusText(t.status) }}
        </span>
      </div>
      <div class="small">{{ t.from_class_name }} → {{ t.to_class_name }}</div>
      <div v-if="t.reason" class="small muted" style="margin-top: 3px">{{ t.reason }}</div>
      <div v-if="isPending(t.status)" class="row" style="margin-top: 6px">
        <button class="fixed primary" @click="transferAct(t.id, 'approve')">同意</button>
        <button class="fixed" @click="transferAct(t.id, 'reject')">拒绝</button>
      </div>
    </div>

    <template v-if="isRoot">
      <div class="sep"></div>
      <h3>创建班级</h3>
      <div class="row">
        <input v-model="state.adminClassName" placeholder="名称，如 高二(5)班" />
        <input v-model="state.adminClassGrade" placeholder="年级" />
      </div>
      <button style="margin-top: 8px" @click="createClass">创建</button>

      <div class="sep"></div>
      <h3>导入名单</h3>
      <label>班级</label>
      <select v-model.number="state.adminSelectedClass">
        <option :value="null">选择班级</option>
        <option v-for="c in state.adminClasses" :key="c.id" :value="c.id">
          {{ c.name }} {{ c.grade }}
        </option>
      </select>
      <label>名单（每行一个姓名）</label>
      <textarea v-model="state.adminRosterText" placeholder="张三&#10;李四"></textarea>
      <button style="margin-top: 8px" @click="importRoster">导入</button>

      <div class="sep"></div>
      <h3>设班级管理员</h3>
      <div class="row">
        <select v-model.number="state.adminSelectedClass">
          <option :value="null">选择班级</option>
          <option v-for="c in state.adminClasses" :key="c.id" :value="c.id">
            {{ c.name }}
          </option>
        </select>
        <input v-model="state.adminUid" placeholder="学生 UID" />
      </div>
      <button style="margin-top: 8px" @click="assignAdmin">设置</button>
    </template>

    <div class="sep"></div>
    <h3>生成重置密钥</h3>
    <div class="row">
      <input v-model="state.adminUid" placeholder="学生 UID" />
      <input v-model="state.adminMinutes" placeholder="有效期（分钟）" />
    </div>
    <button style="margin-top: 8px" @click="createResetKey">生成</button>
    <p v-if="state.resetCode" class="err">重置密钥：{{ state.resetCode }}（只显示一次）</p>

    <template v-if="isRoot">
      <div class="sep"></div>
      <h3>审计日志</h3>
      <div class="row">
        <input
          v-model="state.auditFilter"
          placeholder="按动作前缀筛选，如 auth."
          @keyup.enter="queryAudit"
        />
        <button class="fixed" @click="queryAudit">查询</button>
      </div>
      <div class="row" style="margin-top: 8px; flex-wrap: wrap">
        <button class="fixed" @click="verifyAudit">校验哈希链</button>
        <button class="fixed" @click="exportAudit">导出 CSV</button>
      </div>
      <div v-if="state.auditVerify" style="margin-top: 6px">
        <span v-if="state.auditVerify.ok" class="small muted">
          哈希链完整，已校验 {{ state.auditVerify.checked }} 条
        </span>
        <span v-else class="err">第 {{ state.auditVerify.first_bad }} 条开始不一致</span>
      </div>
      <div class="card" style="max-height: 240px; overflow-y: auto; margin-top: 8px">
        <div v-for="a in state.adminAudit" :key="a.id" class="small" style="padding: 3px 0">
          #{{ a.id }} · {{ formatTime(a.ts) }} · {{ a.action }}
          <span class="muted">{{ a.detail || "" }}</span>
        </div>
        <div v-if="state.adminAudit.length === 0" class="empty">没有记录</div>
      </div>

      <div class="sep"></div>
      <div class="row" style="align-items: baseline">
        <h3 style="flex: 1; margin: 0">备份</h3>
        <button class="link fixed" @click="loadBackups">刷新</button>
      </div>
      <button style="margin-top: 6px" @click="createBackup">立即备份</button>
      <div v-for="b in state.backups" :key="b.name" class="friend-row">
        <div class="grow">
          <div class="account-name">{{ b.name }}</div>
          <div class="account-sub">
            {{ formatTime(b.created_at) }} · {{ (b.bytes / 1024 / 1024).toFixed(2) }} MB
          </div>
        </div>
        <button class="fixed" @click="restore(b.name)">恢复</button>
      </div>
      <div class="hint">
        恢复会在服务端下次启动时执行；当前数据先移到 pre-restore-* 目录。
      </div>

      <div class="sep"></div>
      <h3>发布客户端更新</h3>
      <div class="hint">
        GitHub Releases 由 tag 自动发布；本地更新源可把 manifest.json + package.exe 放进服务端
        data/update/。下面为旧版签名上传（需要 LIAISONYY_UPDATE_PUBKEY）。
      </div>
      <label>版本号</label>
      <input v-model="updVersion" placeholder="0.2.0" />
      <label>最低可用版本</label>
      <input v-model="updMin" placeholder="0.2.0" />
      <label>签名（base64）</label>
      <input v-model="updSig" placeholder="Ed25519 签名" />
      <label>更新包地址（可选，留空使用本机）</label>
      <input v-model="updUrl" placeholder="https://.../liaisonyy.exe" />
      <label>更新包（liaisonyy.exe）</label>
      <input type="file" accept=".exe,application/octet-stream" @change="pickUpdate" />
      <button :disabled="!updData" style="margin-top: 8px" @click="uploadUpdate">
        上传并发布
      </button>
    </template>

    <div class="sep"></div>
    <h3>聊天解密（内部工具）</h3>
    <div class="hint">按设计稿的托管机制恢复/核查；每次查看都会写入审计日志。</div>
    <div class="row">
      <input v-model="state.decryptUid" placeholder="学生 UID" @keyup.enter="loadUserConvs" />
      <button class="fixed" @click="loadUserConvs">加载会话</button>
    </div>
    <div
      v-for="c in state.adminConvs"
      :key="c.id"
      class="friend-row clickable"
      @click="decryptConv(c.id)"
    >
      <div class="grow">
        <div class="account-name">{{ c.title }}</div>
        <div class="account-sub">
          {{ c.kind === "class_group" ? "班级群" : "私聊" }} · #{{ c.id }}
        </div>
      </div>
    </div>
    <div
      v-if="state.adminDecrypted"
      class="card"
      style="max-height: 280px; overflow-y: auto; margin-top: 8px"
    >
      <div
        v-for="m in state.adminDecrypted.messages"
        :key="m.id"
        class="small"
        style="padding: 3px 0"
      >
        <b>{{ m.sender_name }}</b>：{{ m.text }}
        <span class="muted">{{ formatTime(m.created_at) }}</span>
      </div>
      <div v-if="state.adminDecrypted.messages.length === 0" class="empty">没有消息</div>
    </div>

    <template v-if="isRoot && state.adminClasses.length > 0">
      <div class="sep"></div>
      <h3>班级</h3>
      <div v-for="c in state.adminClasses" :key="c.id" class="small" style="padding: 2px 0">
        #{{ c.id }} {{ c.name }} {{ c.grade }}（未注册 {{ c.unclaimed }}）
      </div>
    </template>
  </div>
</template>
