<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const shift = ref(false);
const rows = ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"];

function key(ch: string) {
  emit("update:modelValue", props.modelValue + (shift.value ? ch.toUpperCase() : ch));
  shift.value = false;
}

function back() {
  emit("update:modelValue", props.modelValue.slice(0, -1));
}

function clear() {
  emit("update:modelValue", "");
}
</script>

<template>
  <div class="kbd">
    <div v-for="(r, i) in rows" :key="i" class="kbd-row">
      <button v-for="c in r" :key="c" class="kbd-key" @click="key(c)">
        {{ shift ? c.toUpperCase() : c }}
      </button>
      <button v-if="i === 2" class="kbd-key" @click="back">⌫</button>
    </div>
    <div class="kbd-row">
      <button class="kbd-key" @click="shift = !shift">⇧</button>
      <button class="kbd-key" @click="key('@')">@</button>
      <button class="kbd-key" @click="key('.')">.</button>
      <button class="kbd-key" @click="key('_')">_</button>
      <button class="kbd-key" @click="key('-')">-</button>
      <button class="kbd-key" @click="key(' ')">空格</button>
      <button class="kbd-key" @click="back">⌫</button>
      <button class="kbd-key" @click="clear">清空</button>
    </div>
  </div>
</template>
