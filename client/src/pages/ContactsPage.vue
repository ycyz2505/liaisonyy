<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { state } from "../store";
import type { AccountPublic, FriendRequestView } from "../types";

const myUid = computed(() => state.user?.uid ?? 0);
const pending = computed(() => state.requests.filter((r) => r.status === "pending"));

function accept(r: FriendRequestView) {
  send({ type: "AcceptFriend", data: { request_id: r.id, from_uid: r.from_uid } });
}
function decline(r: FriendRequestView) {
  send({ type: "DeclineFriend", data: { request_id: r.id } });
}
function addFriend(a: AccountPublic) {
  send({ type: "AddFriend", data: { to_uid: a.uid, message: state.friendMsg } });
}
</script>

<template>
  <div>
    <h3 style="margin-top: 0">好友</h3>
    <div v-if="state.friends.length === 0" class="empty">还没有好友</div>
    <div v-for="f in state.friends" :key="f.uid" class="friend-row">
      <div class="avatar">{{ f.display_name.slice(0, 1) || "?" }}</div>
      <div class="grow">
        <div class="account-name">{{ f.display_name }}</div>
        <div class="account-sub">@{{ f.handle }} · UID {{ f.uid }}</div>
      </div>
    </div>

    <div class="sep"></div>
    <h3 style="margin-top: 0">新的朋友</h3>
    <div v-if="pending.length === 0" class="empty">没有待处理的好友申请</div>
    <div v-for="r in pending" :key="r.id" class="request-row">
      <div class="row">
        <div class="grow">
          <b>{{ r.incoming ? r.from_name : r.to_name }}</b>
          <span class="muted small">{{ r.incoming ? " 请求加你" : " 等待对方同意" }}</span>
        </div>
        <template v-if="r.incoming">
          <button class="fixed" @click="accept(r)">同意</button>
          <button class="fixed" @click="decline(r)">拒绝</button>
        </template>
      </div>
      <div v-if="r.incoming && r.message" class="request-msg">附言：{{ r.message }}</div>
    </div>

    <div class="sep"></div>
    <h3 style="margin-top: 0">本班同学</h3>
    <label>附言</label>
    <input v-model="state.friendMsg" placeholder="加好友时附一句话" />
    <div
      v-for="a in state.accounts.filter((x) => x.uid !== myUid)"
      :key="a.uid"
      class="friend-row"
    >
      <div class="avatar">{{ a.display_name.slice(0, 1) || "?" }}</div>
      <div class="grow">
        <div class="account-name">{{ a.display_name }}</div>
        <div class="account-sub">@{{ a.handle }} · UID {{ a.uid }}</div>
      </div>
      <button class="fixed" @click="addFriend(a)">加好友</button>
    </div>
    <div v-if="state.accounts.length <= 1" class="empty">本班还没有其他账号</div>
  </div>
</template>
