<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { rime, type RimeContext } from "./ime/rime";
import { ime, setImeAuto } from "./ime/imeStore";

const loading = ref(true);
const initialized = ref(false);
const error = ref("");
const composition = ref("");
const candidates = ref<{ text: string; comment?: string }[]>([]);
const highlighted = ref(0);
const page = ref(0);
const isLastPage = ref(false);
const english = ref(false);
const shift = ref(false);
const layout = ref<"26" | "9">("26");
const pendingLetters = ref<string[]>([]);
const lastKey = ref("");

const rows26 = ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"];
const digits = [
  { n: "2", letters: "abc" },
  { n: "3", letters: "def" },
  { n: "4", letters: "ghi" },
  { n: "5", letters: "jkl" },
  { n: "6", letters: "mno" },
  { n: "7", letters: "pqrs" },
  { n: "8", letters: "tuv" },
  { n: "9", letters: "wxyz" },
  { n: "1", letters: "，。？！" },
];

async function init() {
  if (initialized.value) return;
  loading.value = true;
  error.value = "";
  try {
    await rime.init();
    initialized.value = true;
    loading.value = false;
  } catch (e) {
    error.value = e instanceof Error ? e.message : "中文输入法加载失败";
    loading.value = false;
  }
}

async function onOpen() {
  await init();
  if (initialized.value) {
    try {
      applyContext(await rime.process("{Escape}"));
    } catch {
      // ignore stale composition reset
    }
  }
}

let unlistenOpen: UnlistenFn | undefined;

onMounted(async () => {
  unlistenOpen = await listen("ime-open", () => {
    void onOpen();
  });
  try {
    if (await getCurrentWindow().isVisible()) void onOpen();
  } catch {
    // window API unavailable
  }
});

onBeforeUnmount(() => {
  unlistenOpen?.();
});

function retry() {
  rime.dispose();
  initialized.value = false;
  void init();
}

function commit(text: string) {
  void emit("ime-commit", text);
}

function backspace() {
  void emit("ime-backspace");
}

function enter() {
  void emit("ime-enter");
}

function startDrag(e: MouseEvent) {
  if ((e.target as HTMLElement).closest("button")) return;
  e.preventDefault();
  void invoke("ime_start_drag");
}

function closeWindow() {
  void invoke("ime_hide");
}

function applyContext(ctx: RimeContext) {
  if (ctx.committed) commit(ctx.committed);
  if (ctx.state === 1) {
    composition.value = [ctx.head ?? "", ctx.body ?? "", ctx.tail ?? ""].join("");
    candidates.value = ctx.candidates ?? [];
    highlighted.value = ctx.highlighted ?? 0;
    page.value = ctx.page ?? 0;
    isLastPage.value = !!ctx.isLastPage;
    return;
  }
  composition.value = "";
  candidates.value = [];
  if (ctx.state === 3 && !ctx.committed) {
    const k = lastKey.value;
    if (k.length === 1) commit(shift.value ? k.toUpperCase() : k);
    else if (k === " ") commit(" ");
    else if (k === "{Return}") enter();
  }
  shift.value = false;
}

function englishKey(k: string) {
  if (k === "{BackSpace}") backspace();
  else if (k === "{Return}") enter();
  else if (k === " ") commit(" ");
  else if (k.length === 1) commit(shift.value ? k.toUpperCase() : k);
  shift.value = false;
}

async function press(k: string) {
  if (english.value) {
    englishKey(k);
    return;
  }
  if (k === "{BackSpace}" && !composition.value) {
    backspace();
    return;
  }
  if ((k === " " || k === "{Return}") && !composition.value) {
    if (k === " ") commit(" ");
    else enter();
    return;
  }
  lastKey.value = k;
  try {
    applyContext(await rime.process(k));
  } catch (e) {
    error.value = e instanceof Error ? e.message : "输入法错误";
  }
}

async function choose(index: number) {
  try {
    applyContext(await rime.select(index));
  } catch (e) {
    error.value = e instanceof Error ? e.message : "输入法错误";
  }
}

async function turnPage(backward: boolean) {
  try {
    applyContext(await rime.changePage(backward));
  } catch (e) {
    error.value = e instanceof Error ? e.message : "输入法错误";
  }
}

async function clear() {
  if (!composition.value) return;
  try {
    applyContext(await rime.process("{Escape}"));
  } catch (e) {
    error.value = e instanceof Error ? e.message : "输入法错误";
  }
}

function pickDigit(d: { n: string; letters: string }) {
  pendingLetters.value = d.letters.split("");
}

function pickLetter(l: string) {
  pendingLetters.value = [];
  void press(l);
}

function apostrophe() {
  void press("'");
}
</script>

<template>
  <div class="ime-window">
    <div class="ime-panel ime-window-panel">
      <div class="ime-head" @mousedown="startDrag">
        <span class="ime-title">中文键盘</span>
        <span class="ime-drag-hint">拖动移动</span>
        <button
          class="ime-auto"
          :class="{ on: ime.auto }"
          @mousedown.prevent
          @click="setImeAuto(!ime.auto)"
        >
          自动
        </button>
        <button class="ime-close" @mousedown.prevent @click="closeWindow">×</button>
      </div>

      <div v-if="loading" class="ime-status">正在加载中文输入法…（首次约几秒）</div>
      <div v-else-if="error" class="ime-status err">
        {{ error }}
        <button @mousedown.prevent @click="retry">重试</button>
      </div>
      <template v-else>
        <div class="ime-candidates">
          <span class="ime-comp">{{ composition }}</span>
          <button
            class="ime-page"
            :disabled="page <= 0"
            @mousedown.prevent
            @click="turnPage(true)"
          >
            ‹
          </button>
          <button
            v-for="(c, i) in candidates"
            :key="i"
            class="ime-candidate"
            :class="{ hl: i === highlighted }"
            @mousedown.prevent
            @click="choose(i)"
          >
            <span class="ime-num">{{ i + 1 }}</span>{{ c.text }}
          </button>
          <span v-if="candidates.length === 0" class="ime-empty">
            {{ composition ? "…" : "输入拼音" }}
          </span>
          <button
            class="ime-page"
            :disabled="isLastPage"
            @mousedown.prevent
            @click="turnPage(false)"
          >
            ›
          </button>
        </div>

        <div class="ime-toolbar">
          <button
            class="ime-tool"
            :class="{ on: english }"
            @mousedown.prevent
            @click="english = !english"
          >
            {{ english ? "EN" : "中" }}
          </button>
          <button
            class="ime-tool"
            @mousedown.prevent
            @click="layout = layout === '26' ? '9' : '26'"
          >
            {{ layout === "26" ? "26键" : "9键" }}
          </button>
          <button class="ime-tool" @mousedown.prevent @click="clear">清空</button>
        </div>

        <div v-if="layout === '26'" class="ime-keys">
          <div v-for="(row, ri) in rows26" :key="ri" class="ime-row">
            <button
              v-for="k in row"
              :key="k"
              class="ime-key"
              @mousedown.prevent
              @click="press(k)"
            >
              {{ shift ? k.toUpperCase() : k }}
            </button>
            <button
              v-if="ri === 2"
              class="ime-key wide"
              @mousedown.prevent
              @click="press('{BackSpace}')"
            >
              ⌫
            </button>
          </div>
          <div class="ime-row">
            <button class="ime-key wide" @mousedown.prevent @click="shift = !shift">⇧</button>
            <button class="ime-key" @mousedown.prevent @click="press('@')">@</button>
            <button class="ime-key" @mousedown.prevent @click="press('.')">.</button>
            <button class="ime-key" @mousedown.prevent @click="press('_')">_</button>
            <button class="ime-key" @mousedown.prevent @click="press('-')">-</button>
            <button class="ime-key space" @mousedown.prevent @click="press(' ')">空格</button>
            <button class="ime-key" @mousedown.prevent @click="press('{BackSpace}')">⌫</button>
            <button class="ime-key" @mousedown.prevent @click="enter">发送</button>
          </div>
        </div>

        <div v-else class="ime-9">
          <div class="ime-9grid">
            <button
              v-for="d in digits"
              :key="d.n"
              class="ime-key ime-digit"
              @mousedown.prevent
              @click="pickDigit(d)"
            >
              <b>{{ d.n }}</b><span>{{ d.letters }}</span>
            </button>
            <button class="ime-key" @mousedown.prevent @click="press(' ')">空格</button>
            <button class="ime-key" @mousedown.prevent @click="press('{BackSpace}')">⌫</button>
            <button class="ime-key" @mousedown.prevent @click="clear">清空</button>
          </div>
          <div v-if="pendingLetters.length" class="ime-letters">
            <button
              v-for="l in pendingLetters"
              :key="l"
              class="ime-key ime-letter"
              @mousedown.prevent
              @click="pickLetter(l)"
            >
              {{ l }}
            </button>
            <button class="ime-key" @mousedown.prevent @click="apostrophe">'</button>
            <button class="ime-key" @mousedown.prevent @click="pendingLetters = []">收起</button>
          </div>
          <div v-else class="ime-letters hint">先按数字键，再选字母</div>
        </div>
      </template>
    </div>
  </div>
</template>
