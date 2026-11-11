// Transport to the TEMISZ server: JSON over POST /rpc. The session is an
// HttpOnly cookie set by the server, so no token is handled here.

import type { AppStatus, CoreError, MfaChallenge } from "./types";

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

export async function command<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
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
  login: (email: string, password: string) => command<AppStatus | MfaChallenge>("login", { username: email, password }),
  loginMfa: (challenge: string, code: string) => command<AppStatus>("login_mfa", { challenge, code }),
  logout: () => command<AppStatus>("logout"),
  touch: () => command<void>("touch"),
  verifyEmail: (token: string) => command<AppStatus>("verify_email", { token }),
  resendVerification: () => command<void>("resend_verification"),
  forgotPassword: (email: string) => command<void>("forgot_password", { email }),
  resetPassword: (token: string, password: string) => command<AppStatus | MfaChallenge>("reset_password", { token, password }),
  acceptInvite: (token: string, name: string, password: string) => command<AppStatus | MfaChallenge>("accept_invite", { token, name, password }),
  totpSetup: () => command<{ secret: string; uri: string; qrSvg: string }>("totp_setup"),
  totpEnable: (code: string) => command<{ recoveryCodes: string[] }>("totp_enable", { code }),
  totpDisable: (password: string) => command<void>("totp_disable", { password }),
  userSendLink: (id: string) => command<void>("user_send_link", { id }),
  securityLog: () => command<SecurityEvent[]>("security_log"),
  tourSeen: () => command<void>("tour_seen"),
  switchOrg: (accountId: string) => command<AppStatus>("switch_org", { accountId }),
  createOrg: (organization: string) => command<AppStatus>("create_org", { organization }),
};

export interface SecurityEvent {
  at: string;
  email: string;
  kind: string;
  ip: string;
  userAgent: string;
}

export function isMfa(x: AppStatus | MfaChallenge): x is MfaChallenge {
  return (x as MfaChallenge).mfa === true;
}

/** Calls a core API method (see crates/lunda-core/src/api.rs). */
export function api<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
  return command<T>("api", { method, args });
}
