<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { formatTime, state } from "../store";

const isRoot = computed(() => state.user?.role === "root");

function refresh() {
  send({ type: "AdminStats" });
  if (isRoot.value) {
    send({ type: "AdminAudit" });
    send({ type: "AdminClasses" });
  }
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
