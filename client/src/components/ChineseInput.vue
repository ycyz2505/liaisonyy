<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref } from "vue";
import {
  clearImeTarget,
  ime,
  openIme,
  setImeTarget,
  type ImeTarget,
} from "../ime/imeStore";

const props = withDefaults(
  defineProps<{ modelValue: string; placeholder?: string; maxlength?: number }>(),
  { placeholder: "", maxlength: undefined },
);
const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "enter"): void;
}>();

const el = ref<HTMLInputElement | null>(null);

function setValue(v: string) {
  if (props.maxlength != null) v = v.slice(0, props.maxlength);
  emit("update:modelValue", v);
}

function onInput(e: Event) {
  setValue((e.target as HTMLInputElement).value);
}

function insert(text: string) {
  const input = el.value;
  const v = props.modelValue;
  if (!input) {
    setValue(v + text);
    return;
  }
  const start = input.selectionStart ?? v.length;
  const end = input.selectionEnd ?? start;
  setValue(v.slice(0, start) + text + v.slice(end));
  nextTick(() => {
    const p = Math.min(start + text.length, props.maxlength ?? Number.MAX_SAFE_INTEGER);
    input.setSelectionRange(p, p);
  });
}

function backspace() {
  const input = el.value;
  const v = props.modelValue;
  if (!input) {
    setValue(v.slice(0, -1));
    return;
  }
  const start = input.selectionStart ?? 0;
  const end = input.selectionEnd ?? start;
  if (start === end && start === 0) return;
  const from = start === end ? start - 1 : start;
  setValue(v.slice(0, from) + v.slice(end));
  nextTick(() => {
    input.setSelectionRange(from, from);
  });
}

const target: ImeTarget = {
  insert,
  backspace,
  enter: () => emit("enter"),
};

function onFocus() {
  setImeTarget(target);
  if (ime.auto) openIme();
}

function openFromButton() {
  el.value?.focus();
  setImeTarget(target);
  openIme();
}

onBeforeUnmount(() => clearImeTarget(target));
</script>

<template>
  <div class="cn-input">
    <div class="cn-input-row">
      <input
        ref="el"
        :value="modelValue"
        :placeholder="placeholder"
        :maxlength="maxlength"
        @input="onInput"
        @focus="onFocus"
        @click="onFocus"
        @keyup.enter="emit('enter')"
      />
      <button
        class="ime-toggle"
        :class="{ on: ime.open && ime.target === target }"
        @mousedown.prevent
        @click="openFromButton"
      >
        中
      </button>
    </div>
  </div>
</template>
