<script setup lang="ts">
import { computed, onMounted } from "vue";
import { send } from "../api";
import { state } from "../store";
import type { AccountPublic } from "../types";

const sorted = computed(() =>
  [...state.accounts].sort((a, b) => b.unread - a.unread || a.uid - b.uid),
);

function refresh() {
  if (state.classId != null) {
    send({ type: "Accounts", data: { class_id: state.classId } });
  }
}

function openAccount(a: AccountPublic) {
  state.loginAccount = a;
  state.loginKey = String(a.uid);
  state.loginKind = a.login_policy === "pin_allowed" ? "pin" : "password";
  state.loginInput = "";
  state.page = "Login";
}

function register() {
  if (state.classId != null) {
    send({ type: "Roster", data: { class_id: state.classId } });
  }
  state.page = "Register";
}

function manualLogin() {
  state.loginAccount = null;
  state.loginKey = "";
  state.loginKind = "password";
  state.loginInput = "";
  state.page = "Login";
}

onMounted(refresh);
</script>

<template>
  <div class="page">
    <div class="row" style="align-items: baseline">
      <h2 style="flex: 1">{{ state.className || "本班账号" }}</h2>
      <button class="link fixed" @click="refresh">刷新</button>
    </div>
    <div class="hint">有未读消息的账号排在最上面；小黑点是未读数。</div>

    <div style="margin-top: 8px">
      <div
        v-for="a in sorted"
        :key="a.uid"
        class="account-row"
        @click="openAccount(a)"
      >
        <div class="avatar">{{ a.display_name.slice(0, 1) || "?" }}</div>
        <div class="account-main">
          <div class="account-name">{{ a.display_name }}</div>
          <div class="account-sub">@{{ a.handle }} · UID {{ a.uid }}</div>
        </div>
        <div v-if="a.unread > 0" class="unread">{{ a.unread > 99 ? "99+" : a.unread }}</div>
      </div>
      <div v-if="sorted.length === 0" class="empty">还没有账号。点下方「注册新账号」。</div>
    </div>

    <div class="bottom-bar">
      <button @click="register">注册新账号</button>
      <button @click="manualLogin">uid/YY 号登录</button>
      <button @click="state.page = 'Settings'">设置</button>
    </div>
  </div>
</template>
