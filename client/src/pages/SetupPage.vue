<script setup lang="ts">
import { send } from "../api";
import { state } from "../store";
import Keyboard from "../components/Keyboard.vue";

function submit() {
  send({
    type: "Setup",
    data: {
      token: state.setupToken.trim(),
      handle: state.setupHandle.trim(),
      display_name: state.setupDisplay.trim(),
      password: state.setupPassword,
    },
  });
}

function copyCode() {
  if (state.recoveryCode) {
    void navigator.clipboard?.writeText(state.recoveryCode);
  }
}

function finishSetup() {
  state.loginKey = state.setupHandle;
  state.loginKind = "password";
  state.loginInput = "";
  state.recoveryCode = "";
  state.page = "Login";
}
</script>

<template>
  <div class="page">
    <h2>初始化服务端</h2>
    <p class="muted small">初始化口令由服务端启动时打印（SETUP TOKEN）</p>
    <label>初始化口令</label>
    <input v-model="state.setupToken" placeholder="8 位数字" />
    <label>总管理员 YY 号</label>
    <input v-model="state.setupHandle" placeholder="例如 admin" />
    <label>显示名称</label>
    <input v-model="state.setupDisplay" placeholder="例如 总管理员" />
    <label>密码（8 位以上，含字母和数字）</label>
    <input v-model="state.setupPassword" type="password" />
    <Keyboard v-model="state.setupPassword" />

    <div v-if="state.recoveryCode" class="card">
      <p class="err">恢复码（只显示一次，请抄下来离线保存）：</p>
      <p class="mono" style="font-size: 18px; letter-spacing: 1px">{{ state.recoveryCode }}</p>
      <button style="margin-top: 8px" @click="copyCode">复制恢复码</button>
      <button class="primary" style="margin-top: 8px" @click="finishSetup">我已保存，去登录</button>
    </div>

    <button v-if="!state.recoveryCode" class="primary" @click="submit">初始化</button>
    <button @click="state.page = 'Welcome'">返回</button>
  </div>
</template>
