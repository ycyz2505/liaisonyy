<script setup lang="ts">
import { send } from "../api";
import { state } from "../store";
import Keyboard from "../components/Keyboard.vue";

function submit() {
  send({
    type: "Reset",
    data: {
      key: state.resetKey.trim(),
      code: state.resetCode.trim(),
      new_password: state.resetNewPassword,
      new_pin: state.resetNewPin.trim() === "" ? null : state.resetNewPin.trim(),
    },
  });
}
</script>

<template>
  <div class="page">
    <h2>重置密码</h2>
    <p class="muted small">向班级管理员/总管理员索取 8 位一次性密钥</p>
    <label>UID 或 YY 号</label>
    <input v-model="state.resetKey" />
    <label>8 位密钥</label>
    <input v-model="state.resetCode" placeholder="8 位数字" />
    <label>新密码（8 位以上，含字母和数字）</label>
    <input v-model="state.resetNewPassword" type="password" />
    <Keyboard v-model="state.resetNewPassword" />
    <label>新 PIN（可选，4-20 位数字；留空则改为仅密码登录）</label>
    <input v-model="state.resetNewPin" type="password" />
    <button class="primary" @click="submit">重置</button>
  </div>
</template>
