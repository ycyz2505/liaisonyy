import { reactive } from "vue";
import { send } from "./api";

const cache = reactive<Record<number, string>>({});
const pending = new Set<number>();

/** Data URL for an uploaded image. Fetches once through the Rust side. */
export function fileUrl(id: number | null | undefined): string {
  if (!id) return "";
  if (!cache[id] && !pending.has(id)) {
    pending.add(id);
    send({ type: "FetchFile", data: { id } });
  }
  return cache[id] ?? "";
}

export function setFileData(id: number, mime: string, data: string): void {
  cache[id] = `data:${mime};base64,${data}`;
  pending.delete(id);
}
