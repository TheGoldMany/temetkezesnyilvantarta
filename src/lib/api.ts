// Transport to the TEMISZ server: JSON over POST /rpc. The session is an
// HttpOnly cookie set by the server, so no token is handled here.

import type { AppStatus, CoreError } from "./types";

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
    const res = await fetch("/rpc", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ cmd, args }),
    });
    if (!res.ok) throw { code: res.status === 403 ? "forbidden" : "transport", message: `HTTP ${res.status}` };
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
  register: (organization: string, name: string, email: string, password: string) =>
    command<AppStatus>("register", { organization, name, email, password }),
  login: (email: string, password: string) => command<AppStatus>("login", { username: email, password }),
  logout: () => command<AppStatus>("logout"),
  touch: () => command<void>("touch"),
};

/** Calls a core API method (see crates/lunda-core/src/api.rs). */
export function api<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
  return command<T>("api", { method, args });
}
