<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { formatTime, state } from "../store";

const unread = computed(() => state.notifications.filter((n) => !n.read).length);

function readAll() {
  send({ type: "MarkNotificationsRead" });
}
function back() {
  state.page = "Main";
  state.tab = "me";
}
</script>

<template>
  <div class="page">
    <div class="row" style="align-items: baseline">
      <h2 style="flex: 1">系统通知</h2>
      <button class="link fixed" @click="readAll">全部标为已读</button>
    </div>
    <div class="hint">{{ unread > 0 ? `${unread} 条未读` : "没有未读通知" }}</div>

    <div v-if="state.notifications.length === 0" class="empty">还没有通知</div>
    <div v-for="n in state.notifications" :key="n.id" class="notice-row">
      <span class="dot" :class="{ on: !n.read }"></span>
      <div class="grow">
        <div>{{ n.text }}</div>
        <div class="small muted">{{ formatTime(n.created_at) }}</div>
      </div>
    </div>

    <button style="margin-top: 12px" @click="back">返回</button>
  </div>
</template>
