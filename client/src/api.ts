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
