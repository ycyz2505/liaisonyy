export interface RimeCandidate {
  text: string;
  comment?: string;
}

export interface RimeContext {
  state: number;
  committed?: string;
  head?: string;
  body?: string;
  tail?: string;
  highlighted?: number;
  candidates?: RimeCandidate[];
  selectLabels?: string[];
  page?: number;
  isLastPage?: boolean;
  updatedSchema?: string;
  updatedOptions?: unknown;
}

interface Pending {
  resolve: (v: unknown) => void;
  reject: (e: Error) => void;
}

/**
 * Thin client for the bundled my_rime worker. The worker is a classic
 * Web Worker with a single-call-at-a-time message protocol.
 */
class RimeClient {
  private worker: Worker | null = null;
  private pending: Pending | null = null;
  private queue: Promise<unknown> = Promise.resolve();
  private initPromise: Promise<void> | null = null;
  private disposed = false;

  private ensureWorker(): Worker {
    if (this.worker) return this.worker;
    const w = new Worker("/myrime/worker.js");
    w.onmessage = (e: MessageEvent) => {
      const d = e.data;
      if (d && d.type === "control") return;
      const p = this.pending;
      this.pending = null;
      if (!p) return;
      if (d && d.type === "success") p.resolve(d.result);
      else p.reject(new Error(d?.error?.message || "输入法错误"));
    };
    w.onerror = (e: ErrorEvent) => {
      const p = this.pending;
      this.pending = null;
      p?.reject(new Error(e.message || "中文输入法加载失败"));
      w.terminate();
      if (this.worker === w) {
        this.worker = null;
        this.initPromise = null;
      }
    };
    this.worker = w;
    return w;
  }

  private call(name: string, ...args: unknown[]): Promise<unknown> {
    const run = () =>
      new Promise<unknown>((resolve, reject) => {
        const w = this.ensureWorker();
        this.pending = { resolve, reject };
        w.postMessage({ name, args, transferableIndices: [] });
      });
    const p = this.queue.then(run, run);
    this.queue = p.catch(() => undefined);
    return p;
  }

  private parse(v: unknown): RimeContext {
    if (typeof v === "string") {
      try {
        return JSON.parse(v) as RimeContext;
      } catch {
        return { state: 0 };
      }
    }
    return (v ?? { state: 0 }) as RimeContext;
  }

  async init(): Promise<void> {
    if (this.initPromise) return this.initPromise;
    this.disposed = false;
    this.initPromise = (async () => {
      this.ensureWorker();
      await this.call("setIME", "luna_pinyin");
      await this.call("setOption", "simplification", 1);
      await this.call("setPageSize", 7);
    })();
    try {
      await this.initPromise;
    } catch (e) {
      this.initPromise = null;
      throw e;
    }
  }

  async process(key: string): Promise<RimeContext> {
    await this.init();
    return this.parse(await this.call("process", key));
  }

  async select(index: number): Promise<RimeContext> {
    await this.init();
    return this.parse(await this.call("selectCandidateOnCurrentPage", index));
  }

  async changePage(backward: boolean): Promise<RimeContext> {
    await this.init();
    return this.parse(await this.call("changePage", backward));
  }

  dispose(): void {
    this.disposed = true;
    const p = this.pending;
    this.pending = null;
    p?.reject(new Error("输入法已重置"));
    this.worker?.terminate();
    this.worker = null;
    this.initPromise = null;
    this.queue = Promise.resolve();
  }

  isDisposed(): boolean {
    return this.disposed;
  }
}

export const rime = new RimeClient();
