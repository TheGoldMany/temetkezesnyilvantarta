// Transport to the Rust core: Tauri IPC in the desktop app, HTTP bridge
// (lunda-devserver) in a plain browser during development and E2E tests.

import type { AppStatus, CoreError } from "./types";

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export class ApiError extends Error {
  code: string;
  params: Record<string, string>;
  constructor(e: CoreError) {
    super(e.message);
    this.code = e.code;
    this.params = e.params ?? {};
  }
}

let onSessionExpired: (() => void) | null = null;
export function setSessionExpiredHandler(fn: () => void) {
  onSessionExpired = fn;
}

async function command<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    if (isTauri) {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<T>(cmd, args);
    }
    const res = await fetch("/rpc", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd, args }),
    });
    const body = await res.json();
    if (body.err) throw body.err;
    return body.ok as T;
  } catch (e: unknown) {
    const err = e as CoreError;
    if (err && typeof err === "object" && "code" in err) {
      if (err.code === "session_expired") onSessionExpired?.();
      throw new ApiError(err);
    }
    throw new ApiError({ code: "transport", message: String(e) });
  }
}

export const shell = {
  status: () => command<AppStatus>("app_status"),
  setupAdmin: (username: string, displayName: string, password: string) =>
    command<AppStatus>("setup_admin", { username, displayName, password }),
  login: (username: string, password: string) => command<AppStatus>("login", { username, password }),
  logout: () => command<AppStatus>("logout"),
  touch: () => command<void>("touch"),
  restoreFresh: (path: string, recoveryKey: string) => command<AppStatus>("restore_fresh", { path, recoveryKey }),
  backupBeforeUpdate: () => command<void>("backup_before_update"),
  isTauri,
};

/** Calls a core API method (see crates/lunda-core/src/api.rs). */
export function api<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
  return command<T>("api", { method, args });
}
