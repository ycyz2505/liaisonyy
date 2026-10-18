<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { showError, state } from "../store";

const pinEnabled = computed(() => state.user?.login_policy === "pin_allowed");

function changePassword() {
  if (state.secNew !== state.secNew2) {
    showError("两次输入的新密码不一致");
    return;
  }
  if (state.secNew.length < 8) {
    showError("新密码至少 8 位");
    return;
  }
  send({
    type: "ChangePassword",
    data: { old_input: state.secOld, new_password: state.secNew },
  });
}

function savePin() {
  if (state.secPin !== state.secPin2) {
    showError("两次输入的 PIN 不一致");
    return;
  }
  if (!/^\d{4,20}$/.test(state.secPin)) {
    showError("PIN 需 4-20 位纯数字");
    return;
  }
  if (!state.secPolicyPassword) {
    showError("请输入当前密码");
    return;
  }
  send({
    type: "SetPin",
    data: { password_input: state.secPolicyPassword, pin: state.secPin },
  });
}

function applyPolicy() {
  if (!state.secPolicyPassword) {
    showError("请输入当前密码");
    return;
  }
  if (state.secPolicy === "pin_allowed") {
    if (!/^\d{4,20}$/.test(state.secPin)) {
      showError("开启 PIN 登录需设置 4-20 位纯数字 PIN");
      return;
    }
    if (state.secPin !== state.secPin2) {
      showError("两次输入的 PIN 不一致");
      return;
    }
    send({
      type: "SetLoginPolicy",
      data: {
        password_input: state.secPolicyPassword,
        policy: "pin_allowed",
        pin: state.secPin,
      },
    });
  } else {
    send({
      type: "SetLoginPolicy",
      data: {
        password_input: state.secPolicyPassword,
        policy: "password_only",
        pin: null,
      },
    });
  }
}
</script>

<template>
  <div class="page">
    <h2>密码与 PIN</h2>
    <div class="hint">
      密码用于找回和修改安全设置；PIN 更方便，但越短越容易被猜到，建议 6 位以上。
    </div>

    <h3>修改密码</h3>
    <label>当前密码</label>
    <input v-model="state.secOld" type="password" autocomplete="current-password" />
    <label>新密码（至少 8 位，含字母和数字）</label>
    <input v-model="state.secNew" type="password" autocomplete="new-password" />
    <label>再次输入新密码</label>
    <input v-model="state.secNew2" type="password" autocomplete="new-password" />
    <button class="primary" @click="changePassword">修改密码</button>

    <div class="sep"></div>
    <h3>登录方式</h3>
    <p class="small">
      当前：{{ pinEnabled ? "PIN 或密码都可以登录" : "仅密码登录" }}
    </p>
    <select v-model="state.secPolicy">
      <option value="pin_allowed">PIN 或密码都可以登录</option>
      <option value="password_only">仅密码登录</option>
    </select>
    <label>当前密码（确认身份）</label>
    <input v-model="state.secPolicyPassword" type="password" autocomplete="current-password" />
    <template v-if="state.secPolicy === 'pin_allowed'">
      <label>PIN（4-20 位纯数字）</label>
      <input v-model="state.secPin" type="password" inputmode="numeric" autocomplete="off" />
      <label>再次输入 PIN</label>
      <input v-model="state.secPin2" type="password" inputmode="numeric" autocomplete="off" />
    </template>
    <button class="primary" @click="applyPolicy">保存登录方式</button>

    <template v-if="pinEnabled">
      <div class="sep"></div>
      <h3>修改 PIN</h3>
      <label>当前密码（确认身份）</label>
      <input v-model="state.secPolicyPassword" type="password" autocomplete="current-password" />
      <label>新 PIN</label>
      <input v-model="state.secPin" type="password" inputmode="numeric" autocomplete="off" />
      <label>再次输入新 PIN</label>
      <input v-model="state.secPin2" type="password" inputmode="numeric" autocomplete="off" />
      <button @click="savePin">修改 PIN</button>
    </template>

    <div class="hint">
      修改密码或 PIN 后，其他设备上的登录会自动退出，需要重新登录。
    </div>

    <button style="margin-top: 12px" @click="state.page = 'Profile'">返回资料页</button>
  </div>
</template>
