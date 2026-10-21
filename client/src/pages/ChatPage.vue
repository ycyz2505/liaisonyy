<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { send, serverNowSec } from "../api";
import { formatTime, newId, state } from "../store";

const listRef = ref<HTMLElement | null>(null);

const convId = computed(() => state.currentConv);
const msgs = computed(() => (convId.value == null ? [] : state.messages[convId.value] ?? []));

function scrollBottom() {
  nextTick(() => {
    if (listRef.value) listRef.value.scrollTop = listRef.value.scrollHeight;
  });
}

function markRead() {
  if (convId.value == null) return;
  const last = msgs.value.reduce((m, x) => Math.max(m, x.id), 0);
  if (last > 0) send({ type: "MarkRead", data: { conv_id: convId.value, last_id: last } });
}

function sendMsg() {
  const text = state.chatInput.trim();
  if (!text || convId.value == null || !state.user) return;
  const client_msg_id = newId();
  const list = state.messages[convId.value] ?? [];
  list.push({
    id: 0,
    sender_uid: state.user.uid,
    sender_name: state.user.display_name,
    text,
    created_at: serverNowSec(),
    mine: true,
    pending: true,
    client_msg_id,
  });
  state.messages[convId.value] = list;
  state.chatInput = "";
  send({ type: "SendMessage", data: { conv_id: convId.value, text, client_msg_id } });
  scrollBottom();
  markRead();
}

function startReport(id: number) {
  if (id <= 0) return;
  state.reportMessageId = id;
  state.reportReason = "";
}

function submitReport() {
  const id = state.reportMessageId;
  const reason = state.reportReason.trim();
  if (!id || !reason) return;
  send({ type: "Report", data: { target_type: "message", target_id: id, reason } });
}

watch(msgs, scrollBottom, { deep: true });
onMounted(() => {
  if (convId.value != null) {
    send({ type: "OpenConversation", data: { conv_id: convId.value } });
    markRead();
  }
  scrollBottom();
});
</script>

<template>
  <div class="chat-page">
    <div ref="listRef" class="chat-list">
      <div
        v-for="m in msgs"
        :key="m.id + ':' + m.client_msg_id"
        class="msg"
        :class="m.mine ? 'mine' : 'other'"
      >
        <div v-if="!m.mine" class="msg-name">{{ m.sender_name }}</div>
        <div class="bubble">{{ m.text }}</div>
        <div class="msg-time">
          {{ formatTime(m.created_at) }}<span v-if="m.pending"> 发送中</span>
          <button
            v-if="!m.mine && !m.pending"
            class="link report-link"
            @click="startReport(m.id)"
          >
            举报
          </button>
        </div>
      </div>
      <div v-if="msgs.length === 0" class="empty">还没有消息</div>
    </div>

    <div class="chat-input">
      <input
        v-model="state.chatInput"
        placeholder="输入消息"
        @keyup.enter="sendMsg"
      />
      <button class="primary" @click="sendMsg">发送</button>
    </div>

    <div v-if="state.reportMessageId" class="modal-mask">
      <div class="modal">
        <h3 style="margin-top: 0">举报消息</h3>
        <textarea
          v-model="state.reportReason"
          maxlength="500"
          placeholder="请说明举报理由（管理员处理时不会看到聊天内容）"
        ></textarea>
        <div class="row" style="margin-top: 8px">
          <button
            @click="
              state.reportMessageId = null;
              state.reportReason = '';
            "
          >
            取消
          </button>
          <button class="primary" :disabled="!state.reportReason.trim()" @click="submitReport">
            提交举报
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
