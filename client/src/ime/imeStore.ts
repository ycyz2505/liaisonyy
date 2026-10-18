import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "../api";

export interface ImeTarget {
  insert(text: string): void;
  backspace(): void;
  enter?: () => void;
}

const AUTO_KEY = "liaisonyy-ime-auto";

function loadAuto(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "0";
  } catch {
    return true;
  }
}

export const ime = reactive({
  open: false,
  auto: loadAuto(),
  target: null as ImeTarget | null,
});

export function setImeTarget(target: ImeTarget): void {
  ime.target = target;
}

export function clearImeTarget(target: ImeTarget): void {
  if (ime.target === target) {
    ime.target = null;
    closeIme();
  }
}

export function openIme(): void {
  ime.open = true;
  if (isTauri) {
    void invoke("ime_show").catch(() => undefined);
  }
}

export function closeIme(): void {
  ime.open = false;
  if (isTauri) {
    void invoke("ime_hide").catch(() => undefined);
  }
}

export function setImeAuto(value: boolean): void {
  ime.auto = value;
  try {
    localStorage.setItem(AUTO_KEY, value ? "1" : "0");
  } catch {
    // ignore storage errors
  }
}

if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key === AUTO_KEY && e.newValue != null) {
      ime.auto = e.newValue !== "0";
    }
  });
}
