<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { state } from "../store";
import Avatar from "../components/Avatar.vue";

const unread = computed(() => state.notifications.filter((n) => !n.read).length);

function openProfile() {
  send({ type: "GetMyProfile" });
  state.page = "Profile";
}

function logout() {
  send({ type: "Logout" });
}
</script>

<template>
  <div>
    <template v-if="state.user">
      <div class="card profile-card" @click="openProfile">
        <Avatar
          :file-id="state.myProfile?.avatar_file_id ?? state.user.avatar_file_id"
          :name="state.user.display_name"
          :size="48"
        />
        <div>
          <div class="account-name" style="font-size: 17px">{{ state.user.display_name }}</div>
          <div class="account-sub">
            UID {{ state.user.uid }} · @{{ state.user.handle }} ·
            {{
              state.user.role === "root"
                ? "总管理员"
                : state.user.role === "class_admin"
                  ? "班级管理员"
                  : "学生"
            }}
          </div>
          <div class="account-sub">
            登录方式：{{ state.user.login_policy === "pin_allowed" ? "PIN 或密码" : "仅密码" }}
          </div>
        </div>
      </div>
    </template>

    <button class="primary" style="margin-top: 10px" @click="openProfile">我的资料</button>
    <button
      style="margin-top: 8px"
      @click="
        state.page = 'Announcements';
        send({ type: 'Announcements' });
      "
    >
      公告
    </button>
    <button
      style="margin-top: 8px"
      @click="
        state.page = 'Notifications';
        send({ type: 'Notifications' });
      "
    >
      系统通知<template v-if="unread > 0">（{{ unread }}）</template>
    </button>
    <button style="margin-top: 8px" @click="state.page = 'Security'">密码与 PIN</button>
    <button style="margin-top: 8px" @click="state.page = 'Settings'">设置</button>
    <button
      v-if="state.user && (state.user.role === 'root' || state.user.role === 'class_admin')"
      style="margin-top: 8px"
      @click="
        state.page = 'Admin';
        send({ type: 'AdminStats' });
        if (state.user?.role === 'root') {
          send({ type: 'AdminAudit' });
          send({ type: 'AdminClasses' });
        }
      "
    >
      管理
    </button>
    <button style="margin-top: 8px" @click="logout">退出登录</button>
  </div>
</template>
