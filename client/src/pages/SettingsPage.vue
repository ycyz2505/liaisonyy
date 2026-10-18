<script setup lang="ts">
import { quitApp, send } from "../api";
import { state } from "../store";

function save() {
  const url = state.serverInput.trim();
  if (!url) return;
  send({ type: "SetServer", data: url });
}

function back() {
  state.page = state.user ? "Main" : "Accounts";
}
</script>

<template>
  <div class="page">
    <h2>设置</h2>
    <label>服务器地址</label>
    <input v-model="state.serverInput" />
    <button class="primary" @click="save">保存服务器地址</button>

    <template v-if="state.user">
      <div class="sep"></div>
      <button @click="send({ type: 'GetMyProfile' }); state.page = 'Profile'">我的资料</button>
      <button style="margin-top: 8px" @click="state.page = 'Security'">密码与 PIN</button>
    </template>

    <div class="sep"></div>
    <h3>隐私与数据说明</h3>
    <p class="small">
      聊天记录加密存储，仅展示在客户端登录后，管理后台不能看到任何聊天记录。
    </p>
    <p class="small">
      PIN 也用于加密；PIN 越短越不安全，建议至少 6 位、最好 8 位以上。
    </p>
    <p class="small">
      服务器是教室电脑，可能关机、还原或断网；聊天记录可能暂时不可用。
    </p>
    <p class="small">不要在聊天中发送身份证号、银行卡号、密码等敏感信息。</p>

    <div class="sep"></div>
    <h3>免责声明</h3>
    <p class="small">
      本系统由同学自建，非学校官方服务；不保证 100% 在线，也不保证聊天记录永不丢失。
      因使用本系统产生的任何损失，开发者与管理员不承担责任。
    </p>

    <button style="margin-top: 10px" @click="back">返回</button>
    <button class="danger" style="margin-top: 8px" @click="quitApp">退出程序</button>
  </div>
</template>
