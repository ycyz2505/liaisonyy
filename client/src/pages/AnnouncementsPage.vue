<script setup lang="ts">
import { computed, onMounted } from "vue";
import { send } from "../api";
import { formatTime, showError, state } from "../store";

const isAdmin = computed(
  () => state.user?.role === "root" || state.user?.role === "class_admin",
);
const isRoot = computed(() => state.user?.role === "root");

onMounted(() => {
  if (!isRoot.value) state.annScope = "class";
  else if (state.annClassId == null) state.annScope = "global";
  send({ type: "Announcements" });
});

function publish() {
  const title = state.annTitle.trim();
  const body = state.annBody.trim();
  if (!title || !body) {
    showError("公告标题和内容不能为空");
    return;
  }
  if (isRoot.value && state.annScope === "class" && state.annClassId == null) {
    showError("请选择班级");
    return;
  }
  send({
    type: "CreateAnnouncement",
    data: {
      scope: isRoot.value ? state.annScope : "class",
      class_id: state.annScope === "class" ? state.annClassId : null,
      title,
      body,
    },
  });
}

function remove(id: number) {
  if (!window.confirm("确定删除这条公告吗？")) return;
  send({ type: "DeleteAnnouncement", data: { id } });
}

function canDelete(a: { scope: string; class_id: number | null }): boolean {
  if (isRoot.value) return true;
  return a.scope === "class" && a.class_id === state.user?.class_id;
}
</script>

<template>
  <div class="page">
    <div class="row" style="align-items: baseline">
      <h2 style="flex: 1">公告</h2>
    </div>

    <template v-if="isAdmin">
      <div class="card">
        <h3 style="margin-top: 0">发布公告</h3>
        <label>范围</label>
        <select v-if="isRoot" v-model="state.annScope">
          <option value="global">全校</option>
          <option value="class">指定班级</option>
        </select>
        <div v-else class="small muted">范围：本班</div>
        <template v-if="isRoot && state.annScope === 'class'">
          <label>班级</label>
          <select v-model.number="state.annClassId">
            <option :value="null">选择班级</option>
            <option v-for="c in state.bootstrap?.classes || []" :key="c.id" :value="c.id">
              {{ c.name }} {{ c.grade }}
            </option>
          </select>
        </template>
        <label>标题</label>
        <input v-model="state.annTitle" maxlength="100" />
        <label>内容</label>
        <textarea v-model="state.annBody" maxlength="5000"></textarea>
        <button class="primary" style="margin-top: 8px" @click="publish">发布</button>
      </div>
      <div class="sep"></div>
    </template>

    <div v-if="state.announcements.length === 0" class="empty">还没有公告</div>
    <div v-for="a in state.announcements" :key="a.id" class="card">
      <div class="row" style="align-items: baseline">
        <b class="grow">{{ a.title }}</b>
        <span class="badge" :class="a.scope === 'global' ? 'badge-warn' : 'badge-ok'">
          {{ a.scope === "global" ? "全校" : a.class_name || "本班" }}
        </span>
      </div>
      <div style="white-space: pre-wrap; margin-top: 6px">{{ a.body }}</div>
      <div class="small muted" style="margin-top: 6px">
        {{ a.created_by_name }} · {{ formatTime(a.created_at) }}
      </div>
      <button
        v-if="isAdmin && canDelete(a)"
        class="link"
        style="margin-top: 4px"
        @click="remove(a.id)"
      >
        删除
      </button>
    </div>
  </div>
</template>
