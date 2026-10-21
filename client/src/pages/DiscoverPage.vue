<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { state } from "../store";

const unread = computed(() => state.notifications.filter((n) => !n.read).length);

function openAnnouncements() {
  state.page = "Announcements";
  send({ type: "Announcements" });
}

function openNotifications() {
  state.page = "Notifications";
  send({ type: "Notifications" });
}
</script>

<template>
  <div class="discover-page">
    <div class="cell-group">
      <button class="cell" @click="openAnnouncements">
        <span class="cell-icon">公</span>
        <span class="cell-label">公告</span>
        <span class="cell-value"></span>
        <span class="cell-chevron">›</span>
      </button>
      <button class="cell" @click="openNotifications">
        <span class="cell-icon">通</span>
        <span class="cell-label">系统通知</span>
        <span class="cell-value">{{ unread > 0 ? unread : "" }}</span>
        <span class="cell-chevron">›</span>
      </button>
    </div>
  </div>
</template>
