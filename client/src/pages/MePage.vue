<script setup lang="ts">
import { send } from "../api";
import { state } from "../store";

function logout() {
  send({ type: "Logout" });
}
</script>

<template>
  <div>
    <template v-if="state.user">
      <div class="card" style="display: flex; align-items: center; gap: 10px">
        <div class="avatar" style="width: 48px; height: 48px; font-size: 20px">
          {{ state.user.display_name.slice(0, 1) || "?" }}
        </div>
        <div>
          <div class="account-name" style="font-size: 17px">{{ state.user.display_name }}</div>
          <div class="account-sub">
            UID {{ state.user.uid }} · @{{ state.user.handle }} ·
            {{
              state.user.role === "root"
                ? "总管理员"
                : state.user.role === "class_admin"
                  ? "班级管理员"
                  : "学生"
            }}
          </div>
          <div class="account-sub">
            登录方式：{{ state.user.login_policy === "pin_allowed" ? "PIN 或密码" : "仅密码" }}
          </div>
        </div>
      </div>
    </template>

    <button style="margin-top: 10px" @click="state.page = 'Settings'">设置</button>
    <button
      v-if="state.user && (state.user.role === 'root' || state.user.role === 'class_admin')"
      style="margin-top: 8px"
      @click="
        state.page = 'Admin';
        send({ type: 'AdminStats' });
        if (state.user?.role === 'root') {
          send({ type: 'AdminAudit' });
          send({ type: 'AdminClasses' });
        }
      "
    >
      管理
    </button>
    <button style="margin-top: 8px" @click="logout">退出登录</button>
  </div>
</template>
