<script setup lang="ts">
import { computed } from "vue";
import { fileUrl } from "../files";

const props = withDefaults(
  defineProps<{ fileId?: number | null; name?: string; size?: number }>(),
  { fileId: null, name: "", size: 38 },
);

const src = computed(() => fileUrl(props.fileId));
const initial = computed(() => (props.name || "?").slice(0, 1));
const fontSize = computed(() => `${Math.max(13, Math.round(props.size * 0.4))}px`);
</script>

<template>
  <div
    class="avatar"
    :style="{
      width: size + 'px',
      height: size + 'px',
      flexBasis: size + 'px',
      fontSize,
    }"
  >
    <img v-if="src" :src="src" alt="" />
    <span v-else>{{ initial }}</span>
  </div>
</template>
