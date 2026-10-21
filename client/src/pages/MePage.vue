<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { showToast, state } from "../store";
import Avatar from "../components/Avatar.vue";

const unread = computed(() => state.notifications.filter((n) => !n.read).length);
const isAdmin = computed(
  () => state.user?.role === "root" || state.user?.role === "class_admin",
);

function openProfile() {
  send({ type: "GetMyProfile" });
  state.page = "Profile";
}

function copyQQ() {
  void navigator.clipboard?.writeText("1130941127");
  showToast("群号已复制：1130941127");
}

function openAnnouncements() {
  state.page = "Announcements";
  send({ type: "Announcements" });
}

function openNotifications() {
  state.page = "Notifications";
  send({ type: "Notifications" });
}

function openAdmin() {
  state.page = "Admin";
  send({ type: "AdminStats" });
  if (state.user?.role === "root") {
    send({ type: "AdminAudit" });
    send({ type: "AdminClasses" });
  }
}

function logout() {
  send({ type: "Logout" });
}
</script>

<template>
  <div class="me-page">
    <template v-if="state.user">
      <button class="me-header" @click="openProfile">
        <Avatar
          :file-id="state.myProfile?.avatar_file_id ?? state.user.avatar_file_id"
          :name="state.user.display_name"
          :size="62"
        />
        <span class="grow">
          <span class="me-name">{{ state.user.display_name }}</span>
          <span class="me-id">YY号：{{ state.user.handle }}</span>
        </span>
        <span class="cell-chevron">›</span>
      </button>
    </template>

    <div class="cell-group">
      <button class="cell" @click="openProfile">
        <span class="cell-label">我的资料</span>
        <span class="cell-chevron">›</span>
      </button>
      <button class="cell" @click="openAnnouncements">
        <span class="cell-label">公告</span>
        <span class="cell-chevron">›</span>
      </button>
      <button class="cell" @click="openNotifications">
        <span class="cell-label">系统通知</span>
        <span class="cell-value">{{ unread > 0 ? unread : "" }}</span>
        <span class="cell-chevron">›</span>
      </button>
    </div>

    <div class="cell-group">
      <button class="cell" @click="state.page = 'Security'">
        <span class="cell-label">密码与 PIN</span>
        <span class="cell-chevron">›</span>
      </button>
      <button class="cell" @click="state.page = 'Settings'">
        <span class="cell-label">设置</span>
        <span class="cell-chevron">›</span>
      </button>
      <button v-if="isAdmin" class="cell" @click="openAdmin">
        <span class="cell-label">管理</span>
        <span class="cell-chevron">›</span>
      </button>
    </div>

    <div class="cell-group">
      <button class="cell" @click="copyQQ">
        <span class="cell-label">liaisonyy 用户 QQ 群</span>
        <span class="cell-value">1130941127</span>
      </button>
    </div>

    <div class="cell-group">
      <button class="cell cell-danger" @click="logout">
        <span class="cell-label">退出登录</span>
      </button>
    </div>

    <div class="me-footer">liaisonyy · 杨村一中内网聊天</div>
  </div>
</template>
