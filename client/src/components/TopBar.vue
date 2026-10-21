<script setup lang="ts">
import { computed } from "vue";
import { panicLogout } from "../api";
import { state } from "../store";

const title = computed(() => {
  switch (state.page) {
    case "Welcome":
      return "liaisonyy";
    case "Accounts":
      return state.className || "选择账号";
    case "Login":
      return "登录";
    case "Register":
      return "注册";
    case "Setup":
      return "初始化";
    case "Reset":
      return "重置密码";
    case "Main":
      return state.tab === "messages"
        ? "消息"
        : state.tab === "contacts"
          ? "通讯录"
          : state.tab === "discover"
            ? "发现"
            : "我";
    case "Chat":
      return state.conversations.find((c) => c.id === state.currentConv)?.title || "聊天";
    case "Settings":
      return "设置";
    case "Profile":
      return "我的资料";
    case "Security":
      return "密码与 PIN";
    case "Notifications":
      return "系统通知";
    case "Announcements":
      return "公告";
    case "Admin":
      return "管理";
    case "UserProfile":
      return state.userProfile?.display_name || "个人资料";
    default:
      return "liaisonyy";
  }
});

const showBack = computed(() => state.page !== "Welcome" && state.page !== "Main");

function back() {
  switch (state.page) {
    case "Accounts":
      state.page = "Welcome";
      break;
    case "Login":
      state.page = state.classId != null ? "Accounts" : "Welcome";
      break;
    case "Register":
      state.page = "Accounts";
      break;
    case "Setup":
      state.page = "Welcome";
      break;
    case "Reset":
      state.page = "Login";
      break;
    case "Chat":
      state.page = "Main";
      state.tab = "messages";
      break;
    case "Settings":
      state.page = state.user ? "Main" : "Accounts";
      if (state.user) state.tab = "me";
      break;
    case "Profile":
    case "Security":
    case "Admin":
      state.page = "Main";
      state.tab = "me";
      break;
    case "Notifications":
    case "Announcements":
      state.page = "Main";
      break;
    case "UserProfile":
      state.page = "Main";
      state.tab = "contacts";
      break;
    default:
      state.page = "Main";
  }
}
</script>

<template>
  <header class="topbar">
    <button v-if="showBack" class="nav-back" title="返回" @click="back">‹</button>
    <span v-else class="nav-spacer"></span>
    <div class="nav-center">
      <span class="nav-title">{{ title }}</span>
      <span v-if="state.offline" class="offline">离线</span>
    </div>
    <button class="nav-close" title="退出登录并隐藏窗口" @click="panicLogout">关闭</button>
  </header>
</template>
