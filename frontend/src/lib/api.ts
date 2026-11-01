export const API_URL =
  process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080/api/v1";

export type ApiErrorBody = { error: { code: string; message: string } };

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

export type Role = "student" | "instructor" | "admin";

export type User = {
  id: string;
  name: string;
  email: string;
  role: Role;
  email_verified: boolean;
  avatar_url: string | null;
  created_at: string;
};

export type Session = { user: User; access_token: string };

// The access token lives only in memory; the refresh token is an httpOnly cookie.
let accessToken: string | null = null;
let refreshing: Promise<Session | null> | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

async function rawFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = new Headers(init.headers);
  if (init.body !== undefined) headers.set("Content-Type", "application/json");
  if (accessToken) headers.set("Authorization", `Bearer ${accessToken}`);
  return fetch(`${API_URL}${path}`, { ...init, headers, credentials: "include" });
}

async function parse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = (await res.json().catch(() => null)) as ApiErrorBody | null;
    throw new ApiError(
      res.status,
      body?.error.code ?? "unknown",
      body?.error.message ?? `Request failed (${res.status})`,
    );
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Exchanges the refresh cookie for a new session. Concurrent callers share one request. */
export function refreshSession(): Promise<Session | null> {
  refreshing ??= rawFetch("/auth/refresh", { method: "POST" })
    .then(async (res) => {
      if (!res.ok) {
        setAccessToken(null);
        return null;
      }
      const session = (await res.json()) as Session;
      setAccessToken(session.access_token);
      return session;
    })
    .catch(() => null)
    .finally(() => {
      refreshing = null;
    });
  return refreshing;
}

/** JSON API call. On 401 it refreshes the session once and retries. */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  let res = await rawFetch(path, init);
  if (res.status === 401 && accessToken && !path.startsWith("/auth/")) {
    if (await refreshSession()) res = await rawFetch(path, init);
  }
  return parse<T>(res);
}

export const post = <T>(path: string, body?: unknown) =>
  api<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) });
