<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { showError, state } from "../store";
import type { AccountPublic, FriendRequestView } from "../types";
import Avatar from "../components/Avatar.vue";

const myUid = computed(() => state.user?.uid ?? 0);
const pending = computed(() => state.requests.filter((r) => r.status === "pending"));
const classGroup = computed(() =>
  state.conversations.find((c) => c.kind === "class_group"),
);

function accept(r: FriendRequestView) {
  send({ type: "AcceptFriend", data: { request_id: r.id, from_uid: r.from_uid } });
}
function decline(r: FriendRequestView) {
  send({ type: "DeclineFriend", data: { request_id: r.id } });
}
function addFriend(a: AccountPublic) {
  send({ type: "AddFriend", data: { to_uid: a.uid, message: state.friendMsg } });
}
function search() {
  const q = state.lookupQuery.trim();
  if (!q) {
    showError("请输入 YY 号或 UID");
    return;
  }
  send({ type: "LookupUser", data: { query: q } });
}
function openProfile(uid: number) {
  send({ type: "GetUserProfile", data: { uid } });
}
function openClassGroup() {
  const c = classGroup.value;
  if (!c) return;
  state.currentConv = c.id;
  state.chatInput = "";
  state.page = "Chat";
  send({ type: "OpenConversation", data: { conv_id: c.id } });
}
</script>

<template>
  <div class="contacts-page">
    <div class="search-bar">
      <input
        v-model="state.lookupQuery"
        placeholder="YY号 / UID"
        @keyup.enter="search"
      />
      <button class="search-btn" @click="search">搜索</button>
    </div>

    <div class="cell-group">
      <button class="cell" @click="openClassGroup">
        <span class="cell-icon">群</span>
        <span class="cell-label">班级群</span>
        <span class="cell-value">{{ classGroup?.title || "暂无" }}</span>
        <span class="cell-chevron">›</span>
      </button>
      <div class="cell cell-static">
        <span class="cell-icon">新</span>
        <span class="cell-label">新的朋友</span>
        <span class="cell-value">{{ pending.length > 0 ? pending.length : "" }}</span>
      </div>
    </div>

    <template v-if="pending.length > 0">
      <div class="section-title">新的朋友</div>
      <div class="cell-group">
        <div v-for="r in pending" :key="r.id" class="cell cell-request">
          <Avatar :name="r.incoming ? r.from_name : r.to_name" :size="42" />
          <span class="grow">
            <span class="cell-label">{{ r.incoming ? r.from_name : r.to_name }}</span>
            <span class="cell-sub">
              {{ r.incoming ? "请求加你" : "等待对方同意" }}
              <template v-if="r.incoming && r.message">：{{ r.message }}</template>
            </span>
          </span>
          <template v-if="r.incoming">
            <button class="mini-btn mini-primary" @click="accept(r)">同意</button>
            <button class="mini-btn" @click="decline(r)">拒绝</button>
          </template>
        </div>
      </div>
    </template>

    <div class="section-title">好友</div>
    <div v-if="state.friends.length === 0" class="empty">还没有好友</div>
    <div v-else class="cell-group">
      <button
        v-for="f in state.friends"
        :key="f.uid"
        class="cell"
        @click="openProfile(f.uid)"
      >
        <Avatar :file-id="f.avatar_file_id" :name="f.display_name" :size="42" />
        <span class="grow">
          <span class="cell-label">{{ f.display_name }}</span>
          <span class="cell-sub">@{{ f.handle }} · UID {{ f.uid }}</span>
        </span>
        <span class="cell-chevron">›</span>
      </button>
    </div>

    <div class="section-title">本班同学</div>
    <div class="cell-group">
      <div
        v-for="a in state.accounts.filter((x) => x.uid !== myUid)"
        :key="a.uid"
        class="cell"
      >
        <button class="cell-main" @click="openProfile(a.uid)">
          <Avatar :file-id="a.avatar_file_id" :name="a.display_name" :size="42" />
          <span class="grow">
            <span class="cell-label">{{ a.display_name }}</span>
            <span class="cell-sub">@{{ a.handle }} · UID {{ a.uid }}</span>
          </span>
        </button>
        <button class="mini-btn" @click="addFriend(a)">加好友</button>
      </div>
    </div>
    <div v-if="state.accounts.length <= 1" class="empty">本班还没有其他账号</div>
  </div>
</template>
