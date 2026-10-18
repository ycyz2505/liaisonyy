<script setup lang="ts">
import { panicLogout } from "../api";
import { state } from "../store";
</script>

<template>
  <header class="topbar">
    <span class="brand">liaisonyy</span>
    <span v-if="state.offline" class="offline">离线</span>
    <span class="spacer"></span>
    <button class="close-btn" @click="panicLogout">关闭</button>
  </header>
</template>
