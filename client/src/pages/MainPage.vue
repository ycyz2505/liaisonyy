<script setup lang="ts">
import { computed } from "vue";
import { send } from "../api";
import { state } from "../store";
import type { ConversationInfo } from "../types";
import ContactsPage from "./ContactsPage.vue";
import MePage from "./MePage.vue";

const convs = computed(() =>
  [...state.conversations].sort((a, b) => {
    const ga = a.kind === "class_group" ? 0 : 1;
    const gb = b.kind === "class_group" ? 0 : 1;
    return ga - gb || b.last_at - a.last_at;
  }),
);

function openConv(c: ConversationInfo) {
  state.currentConv = c.id;
  state.chatInput = "";
  state.page = "Chat";
  send({ type: "OpenConversation", data: { conv_id: c.id } });
}
</script>

<template>
  <div class="main-page">
    <div class="main-body">
      <template v-if="state.tab === 'messages'">
        <div v-for="c in convs" :key="c.id" class="conv-row" @click="openConv(c)">
          <div class="avatar">
            {{ c.kind === "class_group" ? "群" : c.title.slice(0, 1) }}
          </div>
          <div class="conv-title">
            <span v-if="c.kind === 'class_group'" class="group-tag">班级群</span>
            {{ c.title }}
          </div>
          <div class="conv-meta">{{ c.unread > 0 ? "" : "" }}</div>
          <div v-if="c.unread > 0" class="unread">{{ c.unread > 99 ? "99+" : c.unread }}</div>
        </div>
        <div v-if="convs.length === 0" class="empty">还没有会话。去通讯录添加好友吧。</div>
      </template>
      <ContactsPage v-else-if="state.tab === 'contacts'" />
      <MePage v-else />
    </div>

    <div class="tabs">
      <div class="tab" :class="{ active: state.tab === 'messages' }" @click="state.tab = 'messages'">
        消息
      </div>
      <div
        class="tab"
        :class="{ active: state.tab === 'contacts' }"
        @click="
          state.tab = 'contacts';
          send({ type: 'Requests' });
          send({ type: 'Friends' });
        "
      >
        通讯录
      </div>
      <div class="tab" :class="{ active: state.tab === 'me' }" @click="state.tab = 'me'">我</div>
    </div>
  </div>
</template>
