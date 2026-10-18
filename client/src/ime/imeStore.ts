import { reactive } from "vue";

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
  if (ime.target === target) ime.target = null;
}

export function openIme(): void {
  ime.open = true;
}

export function closeIme(): void {
  ime.open = false;
}

export function setImeAuto(value: boolean): void {
  ime.auto = value;
  try {
    localStorage.setItem(AUTO_KEY, value ? "1" : "0");
  } catch {
    // ignore storage errors
  }
}
