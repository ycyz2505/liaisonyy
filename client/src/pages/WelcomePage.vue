<script setup lang="ts">
import { send } from "../api";
import { state } from "../store";
import type { ClassInfo } from "../types";

function connect() {
  const url = state.serverInput.trim();
  if (!url) return;
  send({ type: "SetServer", data: url });
  send({ type: "Bootstrap" });
}

function choose(c: ClassInfo) {
  state.classId = c.id;
  state.className = c.name;
  state.page = "Accounts";
  send({ type: "Accounts", data: { class_id: c.id } });
}

function adminLogin() {
  state.loginAccount = null;
  state.loginKey = "";
  state.loginKind = "password";
  state.loginInput = "";
  state.page = "Login";
}
</script>

<template>
  <div class="page">
    <h2>欢迎使用 liaisonyy</h2>
    <p class="muted">杨村一中内网聊天系统</p>
    <label>服务端地址</label>
    <input v-model="state.serverInput" placeholder="https://127.0.0.1:47820" />
    <button class="primary" @click="connect">连接服务端</button>

    <template v-if="state.bootstrap">
      <p class="muted small">
        {{ state.bootstrap.school_name }} · {{ state.bootstrap.server_name }} · v{{ state.bootstrap.version }}
      </p>
      <template v-if="state.bootstrap.setup_required">
        <p class="warn">服务端尚未初始化</p>
        <button @click="state.page = 'Setup'">初始化服务端</button>
      </template>
      <template v-else>
        <label>选择班级</label>
        <button
          v-for="c in state.bootstrap.classes"
          :key="c.id"
          class="class-btn"
          @click="choose(c)"
        >
          <span>{{ c.name }}</span>
          <span class="muted small">{{ c.grade }}</span>
        </button>
        <p v-if="state.bootstrap.classes.length === 0" class="muted small">
          还没有班级，请联系总管理员。
        </p>
      </template>
    </template>

    <div class="sep"></div>
    <button class="link" @click="adminLogin">uid/YY 号登录（管理员）</button>
  </div>
</template>
