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

export function getAccessToken(): string | null {
  return accessToken;
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

const send =
  (method: string) =>
  <T>(path: string, body?: unknown) =>
    api<T>(path, { method, body: body === undefined ? undefined : JSON.stringify(body) });

export const post = send("POST");
export const put = send("PUT");
export const patch = send("PATCH");
export const del = send("DELETE");

// ---------- domain types ----------

export type Category = { id: string; parent_id: string | null; name: string; slug: string };

export type CourseStatus = "draft" | "pending" | "published" | "rejected";
export type Level = "all" | "beginner" | "intermediate" | "expert";
export type Currency = "USD" | "KHR";

export type Course = {
  id: string;
  instructor_id: string;
  category_id: string | null;
  title: string;
  slug: string;
  subtitle: string;
  description: string;
  level: Level;
  language: string;
  /** Minor units: USD cents, KHR riel. */
  price_minor: number;
  currency: Currency;
  thumbnail_url: string | null;
  outcomes: string[];
  requirements: string[];
  status: CourseStatus;
  rejection_reason: string | null;
  created_at: string;
  updated_at: string;
};

export type LectureKind = "video" | "article";

export type Lecture = {
  id: string;
  section_id: string;
  title: string;
  kind: LectureKind;
  position: number;
  is_preview: boolean;
  article_body: string;
  duration_sec: number;
  video_asset_id: string | null;
};

export type Section = { id: string; title: string; position: number; lectures: Lecture[] };

export type VideoStatus = "uploading" | "processing" | "ready" | "failed";
export type VideoInfo = { asset_id: string; status: VideoStatus; duration_sec: number; error: string | null };

export function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  const mm = String(m).padStart(h ? 2 : 1, "0");
  return `${h ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
}

export type Page<T> = { data: T[]; meta: { page: number; per_page: number; total: number } };

export type CourseCard = {
  id: string;
  slug: string;
  title: string;
  subtitle: string;
  thumbnail_url: string | null;
  category_id: string | null;
  level: Level;
  language: string;
  price_minor: number;
  currency: Currency;
  instructor_id: string;
  instructor_name: string;
  lecture_count: number;
  duration_sec: number;
  published_at: string | null;
};

export type PublicLecture = {
  id: string;
  title: string;
  kind: LectureKind;
  duration_sec: number;
  is_preview: boolean;
  article_body?: string;
  preview_asset_id?: string;
};

export type CoursePage = CourseCard & {
  description: string;
  outcomes: string[];
  requirements: string[];
  instructor: { id: string; name: string; avatar_url: string | null; headline: string; bio: string };
  curriculum: { id: string; title: string; lectures: PublicLecture[] }[];
};

export const LEVEL_LABELS: Record<Level, string> = {
  all: "All levels",
  beginner: "Beginner",
  intermediate: "Intermediate",
  expert: "Expert",
};

export const LANGUAGE_LABELS: Record<string, string> = { en: "English", km: "Khmer" };

export type InstructorProfile = { user_id: string; headline: string; bio: string; website: string | null };

/** Formats a minor-unit amount, e.g. 1999 USD -> "$19.99", 40000 KHR -> "40,000៛". */
export function formatPrice(minor: number, currency: Currency): string {
  if (minor === 0) return "Free";
  return currency === "USD"
    ? `$${(minor / 100).toFixed(2)}`
    : `${minor.toLocaleString("en-US")}៛`;
}
