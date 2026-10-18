import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Cmd, Ev, Settings } from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function send(cmd: Cmd): Promise<void> {
  if (!isTauri) {
    console.log("[liaisonyy] cmd", cmd);
    return;
  }
  await invoke("send_cmd", { cmd });
}

export async function onEv(cb: (ev: Ev) => void): Promise<UnlistenFn> {
  if (!isTauri) return () => {};
  return listen<Ev>("liaisonyy-ev", (event) => cb(event.payload));
}

export async function panicLogout(): Promise<void> {
  if (isTauri) await invoke("panic_logout");
}

export async function quitApp(): Promise<void> {
  if (isTauri) await invoke("quit_app");
}

export async function getSettings(): Promise<Settings> {
  if (isTauri) return await invoke<Settings>("get_settings");
  return { server: "https://127.0.0.1:47820", class_id: null, class_name: "", last_uid: null };
}

let serverTimeMs = 0;
let perfAnchor = 0;

/** Anchor the UI clock to the server's network-synced time. */
export async function syncServerTime(): Promise<void> {
  if (!isTauri) {
    serverTimeMs = Date.now();
    perfAnchor = performance.now();
    return;
  }
  try {
    const t = await invoke<number>("get_server_time");
    serverTimeMs = t * 1000;
    perfAnchor = performance.now();
  } catch {
    // keep the previous anchor; the next poll will refresh it
  }
}

/** Monotonic server time in milliseconds. Local wall-clock changes do not affect it. */
export function serverNowMs(): number {
  if (perfAnchor === 0) return Date.now();
  return serverTimeMs + (performance.now() - perfAnchor);
}

export function serverNowSec(): number {
  return Math.floor(serverNowMs() / 1000);
}
