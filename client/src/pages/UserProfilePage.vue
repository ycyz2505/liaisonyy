<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { fileUrl } from "../files";
import { state } from "../store";
import Avatar from "../components/Avatar.vue";

const p = computed(() => state.userProfile);
const bg = computed(() => fileUrl(p.value?.background_file_id));
const bgStyle = computed(() =>
  bg.value ? { backgroundImage: `url(${bg.value})` } : {},
);
const isSelf = computed(() => p.value?.uid === state.user?.uid);

const roleText = computed(() => {
  const r = p.value?.role;
  return r === "root" ? "总管理员" : r === "class_admin" ? "班级管理员" : "学生";
});

function addFriend() {
  if (!p.value) return;
  send({ type: "AddFriend", data: { to_uid: p.value.uid, message: state.friendMsg } });
}
function deleteFriend() {
  if (!p.value) return;
  if (!window.confirm(`确定删除好友「${p.value.display_name}」吗？聊天记录仍会保留。`)) return;
  send({ type: "DeleteFriend", data: { uid: p.value.uid } });
}
function block() {
  if (!p.value) return;
  if (!window.confirm(`确定拉黑「${p.value.display_name}」吗？对方将无法给你发消息。`)) return;
  send({ type: "BlockUser", data: { uid: p.value.uid } });
}
function unblock() {
  if (!p.value) return;
  send({ type: "UnblockUser", data: { uid: p.value.uid } });
}
</script>

<template>
  <div class="page" v-if="p">
    <div
      class="profile-bg"
      :style="bgStyle"
    >
      <div class="profile-head">
        <Avatar :file-id="p.avatar_file_id" :name="p.display_name" :size="64" />
        <div class="grow">
          <div class="profile-name">{{ p.display_name }}</div>
          <div class="account-sub">
            UID {{ p.uid }} · @{{ p.handle }} · {{ roleText }}
          </div>
          <div class="account-sub">
            {{ p.class_name || "未分班" }} · 真实姓名：{{ p.real_name }}
          </div>
        </div>
      </div>
    </div>

    <div v-if="p.is_blocked" class="warn" style="margin-top: 10px">
      你已拉黑该同学，双方无法发送消息。
    </div>

    <div v-if="p.signature" class="card">
      <div class="small muted">个性签名</div>
      <div>{{ p.signature }}</div>
    </div>
    <div v-if="p.bio" class="card">
      <div class="small muted">个人简介</div>
      <div style="white-space: pre-wrap">{{ p.bio }}</div>
    </div>
    <div v-if="p.gender || p.birthday || p.socials_json" class="card">
      <div v-if="p.gender" class="profile-line">
        <span class="muted small">性别</span> {{ p.gender }}
      </div>
      <div v-if="p.birthday" class="profile-line">
        <span class="muted small">生日</span> {{ p.birthday }}
      </div>
      <div v-if="p.socials_json" class="profile-line">
        <span class="muted small">社交账号</span>
        <span style="white-space: pre-wrap">{{ p.socials_json }}</span>
      </div>
    </div>

    <template v-if="isSelf">
      <button class="primary" @click="send({ type: 'GetMyProfile' }); state.page = 'Profile'">
        编辑我的资料
      </button>
    </template>
    <template v-else>
      <button v-if="!p.is_friend" class="primary" @click="addFriend">加好友</button>
      <button v-else @click="deleteFriend">删除好友</button>
      <button v-if="!p.is_blocked" class="danger" @click="block">拉黑</button>
      <button v-else @click="unblock">解除拉黑</button>
    </template>
  </div>
  <div class="page" v-else>
    <div class="empty">没有找到这位同学</div>
  </div>
</template>
