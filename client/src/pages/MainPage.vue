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

const tabTitle = computed(() =>
  state.tab === "messages" ? "消息" : state.tab === "contacts" ? "通讯录" : "我",
);

function openConv(c: ConversationInfo) {
  state.currentConv = c.id;
  state.chatInput = "";
  state.page = "Chat";
  send({ type: "OpenConversation", data: { conv_id: c.id } });
}

function openContacts() {
  state.tab = "contacts";
  send({ type: "Requests" });
  send({ type: "Friends" });
}
</script>

<template>
  <div class="main-page">
    <div class="main-body">
      <h2 class="large-title">{{ tabTitle }}</h2>

      <template v-if="state.tab === 'messages'">
        <div v-for="c in convs" :key="c.id" class="conv-row" @click="openConv(c)">
          <div class="avatar">
            {{ c.kind === "class_group" ? "群" : c.title.slice(0, 1) }}
          </div>
          <div class="conv-title">
            <span v-if="c.kind === 'class_group'" class="group-tag">班级群</span>
            {{ c.title }}
          </div>
          <div v-if="c.unread > 0" class="unread">{{ c.unread > 99 ? "99+" : c.unread }}</div>
        </div>
        <div v-if="convs.length === 0" class="empty">还没有会话。去通讯录添加好友吧。</div>
      </template>
      <ContactsPage v-else-if="state.tab === 'contacts'" />
      <MePage v-else />
    </div>

    <div class="tabs" role="tablist">
      <button
        class="tab"
        :class="{ active: state.tab === 'messages' }"
        role="tab"
        @click="state.tab = 'messages'"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 11.5a8.4 8.4 0 0 1-8.5 8.5 9 9 0 0 1-3.9-.9L3 21l1.9-5.1A8.5 8.5 0 1 1 21 11.5Z" />
        </svg>
        <span>消息</span>
      </button>
      <button
        class="tab"
        :class="{ active: state.tab === 'contacts' }"
        role="tab"
        @click="openContacts"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
          <circle cx="9" cy="7" r="4" />
          <path d="M22 21v-2a4 4 0 0 0-3-3.9" />
          <path d="M16 3.1a4 4 0 0 1 0 7.8" />
        </svg>
        <span>通讯录</span>
      </button>
      <button
        class="tab"
        :class="{ active: state.tab === 'me' }"
        role="tab"
        @click="state.tab = 'me'"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="8" r="4" />
          <path d="M4 21v-1a8 8 0 0 1 16 0v1" />
        </svg>
        <span>我</span>
      </button>
    </div>
  </div>
</template>
