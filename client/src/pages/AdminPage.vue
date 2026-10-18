<script setup lang="ts">
import { computed, onMounted } from "vue";
import { send } from "../api";
import { formatTime, state } from "../store";
import type { ReportDto } from "../types";

const isRoot = computed(() => state.user?.role === "root");

function refresh() {
  send({ type: "AdminStats" });
  send({ type: "AdminReports" });
  if (isRoot.value) {
    send({ type: "AdminAudit" });
    send({ type: "AdminClasses" });
  }
}

onMounted(refresh);

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

function back() {
  state.page = "Main";
}
</script>

<template>
  <div class="page">
    <div class="row" style="align-items: baseline">
      <h2 style="flex: 1">管理</h2>
      <button class="link fixed" @click="back">返回</button>
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

    <template v-if="isRoot && state.adminAudit.length > 0">
      <div class="sep"></div>
      <h3>最近审计</h3>
      <div class="card" style="max-height: 220px; overflow-y: auto">
        <div v-for="a in state.adminAudit" :key="a.id" class="small" style="padding: 3px 0">
          {{ formatTime(a.ts) }} · {{ a.action }}
          <span class="muted">{{ a.detail || "" }}</span>
        </div>
      </div>
    </template>

    <template v-if="isRoot && state.adminClasses.length > 0">
      <div class="sep"></div>
      <h3>班级</h3>
      <div v-for="c in state.adminClasses" :key="c.id" class="small" style="padding: 2px 0">
        #{{ c.id }} {{ c.name }} {{ c.grade }}（未注册 {{ c.unclaimed }}）
      </div>
    </template>
  </div>
</template>
