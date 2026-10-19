<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { getSettings, hideWindow, onEv, quitApp, send, syncServerTime } from "./api";
import { handleEv, state } from "./store";
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
  state.updateGate = "checking";
  state.updateError = "";
  send({ type: "CheckUpdate" });
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
  if (!u || state.updateApplying) return;
  state.updateApplying = true;
  state.updateProgress = "正在准备更新…";
  state.error = "";
  send({ type: "ApplyUpdate", data: { url: u.url, sha256: u.sha256 } });
}

function retryUpdate() {
  state.updateGate = "checking";
  state.updateError = "";
  state.updateApplying = false;
  state.updateProgress = "";
  state.error = "";
  send({ type: "CheckUpdate" });
}
</script>

<template>
  <div class="app">
    <div v-if="state.updateGate !== 'ready'" class="update-gate">
      <div class="update-card">
        <div class="update-brand">liaisonyy</div>
        <template v-if="state.updateGate === 'checking'">
          <p class="update-status">正在检查更新…</p>
          <div class="update-dots"><span></span><span></span><span></span></div>
        </template>
        <template v-else-if="state.updateGate === 'required'">
          <h2>发现新版本 {{ state.updateInfo?.version }}</h2>
          <p>必须更新成功后才能进入 liaisonyy。</p>
          <template v-if="state.updateApplying">
            <p class="update-status">{{ state.updateProgress || "正在更新…" }}</p>
            <div class="update-dots"><span></span><span></span><span></span></div>
            <p class="hint">更新过程中请保持网络连接，不要关闭程序。</p>
          </template>
          <template v-else>
            <button class="primary" @click="applyUpdate">下载并更新</button>
            <p class="hint">下载会自动尝试直连与 GitHub 加速源；请保持网络连接。</p>
          </template>
          <p v-if="state.error" class="err">{{ state.error }}</p>
        </template>
        <template v-else>
          <h2>检查更新失败</h2>
          <p class="err">{{ state.updateError || "无法连接更新服务器" }}</p>
          <button class="primary" @click="retryUpdate">重试</button>
          <button @click="quitApp">退出</button>
          <p class="hint">启动时必须成功检查更新；请检查网络后重试。</p>
        </template>
        <button class="link update-close" @click="hideWindow">关闭窗口</button>
        <p class="hint">关闭后程序仍在后台运行，点击托盘图标可随时重新打开。</p>
      </div>
    </div>

    <template v-else>
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
      <div v-if="state.toast" class="toast">{{ state.toast }}</div>
      <div v-if="state.error" class="error-bar">
        <span>{{ state.error }}</span>
        <button class="link" @click="state.error = ''">×</button>
      </div>
    </template>
  </div>
</template>
