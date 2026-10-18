<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { getSettings, onEv, send, syncServerTime } from "./api";
import { handleEv, state } from "./store";
import { ime } from "./ime/imeStore";
import FloatingKeyboard from "./components/FloatingKeyboard.vue";
import TopBar from "./components/TopBar.vue";
import WelcomePage from "./pages/WelcomePage.vue";
import SetupPage from "./pages/SetupPage.vue";
import AccountsPage from "./pages/AccountsPage.vue";
import LoginPage from "./pages/LoginPage.vue";
import RegisterPage from "./pages/RegisterPage.vue";
import MainPage from "./pages/MainPage.vue";
import ChatPage from "./pages/ChatPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import AdminPage from "./pages/AdminPage.vue";
import ResetPage from "./pages/ResetPage.vue";
import ProfilePage from "./pages/ProfilePage.vue";
import SecurityPage from "./pages/SecurityPage.vue";
import UserProfilePage from "./pages/UserProfilePage.vue";
import NotificationsPage from "./pages/NotificationsPage.vue";
import AnnouncementsPage from "./pages/AnnouncementsPage.vue";

let unlisten: (() => void) | undefined;
let timeTimer: number | undefined;
let retryTimer: number | undefined;
let noticeTimer: number | undefined;

onMounted(async () => {
  unlisten = await onEv(handleEv);
  const s = await getSettings();
  state.serverInput = s.server;
  state.classId = s.class_id;
  state.className = s.class_name;
  state.page = s.class_id != null ? "Accounts" : "Welcome";
  await syncServerTime();
  retryTimer = window.setTimeout(syncServerTime, 2500);
  timeTimer = window.setInterval(syncServerTime, 10 * 60 * 1000);
  noticeTimer = window.setInterval(() => {
    if (state.user) send({ type: "Notifications" });
  }, 60 * 1000);
  send({ type: "Bootstrap" });
  if (s.class_id != null) {
    send({ type: "Accounts", data: { class_id: s.class_id } });
  }
});

onBeforeUnmount(() => {
  unlisten?.();
  if (timeTimer) window.clearInterval(timeTimer);
  if (retryTimer) window.clearTimeout(retryTimer);
  if (noticeTimer) window.clearInterval(noticeTimer);
});

function applyUpdate() {
  const u = state.updateInfo;
  if (!u) return;
  send({ type: "ApplyUpdate", data: { url: u.url, sha256: u.sha256 } });
}
</script>

<template>
  <div class="app">
    <TopBar />
    <main
      class="content"
      :class="{ 'content-fixed': state.page === 'Main' || state.page === 'Chat' }"
    >
      <WelcomePage v-if="state.page === 'Welcome'" />
      <SetupPage v-else-if="state.page === 'Setup'" />
      <AccountsPage v-else-if="state.page === 'Accounts'" />
      <LoginPage v-else-if="state.page === 'Login'" />
      <RegisterPage v-else-if="state.page === 'Register'" />
      <MainPage v-else-if="state.page === 'Main'" />
      <ChatPage v-else-if="state.page === 'Chat'" />
      <SettingsPage v-else-if="state.page === 'Settings'" />
      <AdminPage v-else-if="state.page === 'Admin'" />
      <ResetPage v-else-if="state.page === 'Reset'" />
      <ProfilePage v-else-if="state.page === 'Profile'" />
      <SecurityPage v-else-if="state.page === 'Security'" />
      <UserProfilePage v-else-if="state.page === 'UserProfile'" />
      <NotificationsPage v-else-if="state.page === 'Notifications'" />
      <AnnouncementsPage v-else-if="state.page === 'Announcements'" />
    </main>
    <FloatingKeyboard v-if="ime.open" />
    <div v-if="state.toast" class="toast">{{ state.toast }}</div>
    <div v-if="state.error" class="error-bar">
      <span>{{ state.error }}</span>
      <button class="link" @click="state.error = ''">×</button>
    </div>

    <div v-if="state.updateInfo" class="modal-mask" style="z-index: 30">
      <div class="modal">
        <h3 style="margin-top: 0">发现新版本 {{ state.updateInfo.version }}</h3>
        <p class="small">
          {{
            state.updateInfo.mandatory
              ? "当前版本已停止支持，必须更新后才能继续使用。"
              : "建议更新到最新版本。"
          }}
        </p>
        <button class="primary" @click="applyUpdate">下载并更新</button>
        <button
          v-if="!state.updateInfo.mandatory"
          style="margin-top: 8px"
          @click="state.updateInfo = null"
        >
          稍后
        </button>
      </div>
    </div>
  </div>
</template>
