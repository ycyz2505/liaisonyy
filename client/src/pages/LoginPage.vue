<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { state } from "../store";
import Keyboard from "../components/Keyboard.vue";

const canPin = computed(
  () => state.loginAccount == null || state.loginAccount.login_policy === "pin_allowed",
);

const pinKeys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "清除", "0", "⌫"];

function press(k: string) {
  if (k === "清除") state.loginInput = "";
  else if (k === "⌫") state.loginInput = state.loginInput.slice(0, -1);
  else state.loginInput += k;
}

function login() {
  send({
    type: "Login",
    data: {
      key: state.loginKey.trim(),
      kind: state.loginKind,
      input: state.loginInput,
    },
  });
}

function forgot() {
  state.resetKey = state.loginKey;
  state.resetCode = "";
  state.resetNewPassword = "";
  state.resetNewPin = "";
  state.page = "Reset";
}
</script>

<template>
  <div class="page">
    <h2>登录</h2>
    <template v-if="state.loginAccount">
      <div class="card" style="display: flex; align-items: center; gap: 10px">
        <div class="avatar">{{ state.loginAccount.display_name.slice(0, 1) || "?" }}</div>
        <div>
          <div class="account-name">{{ state.loginAccount.display_name }}</div>
          <div class="account-sub">
            UID {{ state.loginAccount.uid }} · @{{ state.loginAccount.handle }}
          </div>
        </div>
      </div>
    </template>
    <template v-else>
      <label>UID 或 YY 号</label>
      <input v-model="state.loginKey" placeholder="例如 12 或 zhangsan" />
    </template>

    <div class="row" style="margin-top: 8px">
      <button
        :class="{ primary: state.loginKind === 'pin' }"
        :disabled="!canPin"
        @click="state.loginKind = 'pin'"
      >
        PIN
      </button>
      <button
        :class="{ primary: state.loginKind === 'password' }"
        @click="state.loginKind = 'password'"
      >
        密码
      </button>
    </div>

    <input
      v-model="state.loginInput"
      type="password"
      placeholder="可多输入几位，包含正确内容即可"
      style="margin-top: 8px"
      @keyup.enter="login"
    />

    <div v-if="state.loginKind === 'pin'" class="pin-pad">
      <button v-for="k in pinKeys" :key="k" @click="press(k)">{{ k }}</button>
    </div>
    <Keyboard v-else v-model="state.loginInput" />

    <button class="primary" style="margin-top: 10px" @click="login">登录</button>
    <div class="row">
      <button class="link" @click="forgot">忘记密码</button>
      <button class="link" @click="state.page = 'Accounts'">返回账号列表</button>
    </div>
    <p class="hint">PIN 也用于加密，建议至少 6 位、最好 8 位以上。</p>
  </div>
</template>
