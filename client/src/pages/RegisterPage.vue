<script setup lang="ts">
import { computed, onMounted } from "vue";
import { send } from "../api";
import { resetRegisterForm, state } from "../store";
import Keyboard from "../components/Keyboard.vue";

const available = computed(() => state.roster.filter((r) => !r.claimed));

function submit() {
  if (state.selectedRoster == null) {
    state.error = "请先在名单中选择自己的名字";
    return;
  }
  const entry = state.roster.find((r) => r.id === state.selectedRoster);
  if (!entry || state.classId == null) {
    state.error = "名单或班级信息缺失";
    return;
  }
  send({
    type: "Register",
    data: {
      class_id: state.classId,
      roster_id: entry.id,
      real_name: entry.real_name,
      handle: state.regHandle.trim(),
      display_name: state.regDisplay.trim(),
      password: state.regPassword,
      login_policy: state.regPolicy,
      pin: state.regPolicy === "pin_allowed" ? state.regPin : null,
    },
  });
}

onMounted(() => {
  resetRegisterForm();
  if (state.classId != null) {
    send({ type: "Roster", data: { class_id: state.classId } });
  }
});
</script>

<template>
  <div class="page">
    <h2>注册账号</h2>
    <p class="muted small">{{ state.className }}</p>

    <label>选择自己的名字</label>
    <div class="card" style="max-height: 180px; overflow-y: auto">
      <button
        v-for="r in available"
        :key="r.id"
        class="class-btn"
        :class="{ primary: state.selectedRoster === r.id }"
        style="margin: 4px 0"
        @click="state.selectedRoster = r.id"
      >
        <span>{{ r.real_name }}</span>
        <span v-if="r.student_no" class="small">{{ r.student_no }}</span>
      </button>
      <div v-if="available.length === 0" class="empty">名单加载中，或没有可注册的名字。</div>
    </div>

    <label>YY 号（3-20 位，字母数字下划线，不能纯数字）</label>
    <input v-model="state.regHandle" placeholder="类似微信号，唯一" />
    <label>用户名（可中文，可重复）</label>
    <input v-model="state.regDisplay" placeholder="聊天里显示的名字" />
    <label>密码（8 位以上，含字母和数字）</label>
    <input v-model="state.regPassword" type="password" />
    <Keyboard v-model="state.regPassword" />

    <label>登录方式</label>
    <div class="row">
      <button
        :class="{ primary: state.regPolicy === 'pin_allowed' }"
        @click="state.regPolicy = 'pin_allowed'"
      >
        只用 PIN 登录
      </button>
      <button
        :class="{ primary: state.regPolicy === 'password_only' }"
        @click="state.regPolicy = 'password_only'"
      >
        必须用密码登录
      </button>
    </div>
    <p class="hint">
      只用 PIN 更方便，但 PIN 也用于加密；PIN 越短越不安全，建议至少 6 位、最好 8 位以上。
      必须用密码更安全，但每次都要输密码。
    </p>

    <template v-if="state.regPolicy === 'pin_allowed'">
      <label>PIN（4-20 位纯数字）</label>
      <input v-model="state.regPin" type="password" />
    </template>

    <button class="primary" @click="submit">注册</button>
    <button @click="state.page = 'Accounts'">返回</button>
  </div>
</template>
