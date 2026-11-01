"use client";

import { usePathname, useRouter } from "next/navigation";
import { useEffect } from "react";
import type { Role } from "@/lib/api";
import { useAuth } from "@/lib/auth";

function Loading() {
  return (
    <div className="flex flex-1 items-center justify-center py-24 text-sm text-muted-foreground">
      Loading…
    </div>
  );
}

/**
 * Client-side route guard. The refresh cookie lives on the API origin, so the
 * Next.js server can't see the session; the API still enforces every rule.
 */
export function RequireAuth({
  children,
  roles,
}: {
  children: React.ReactNode;
  /** Restrict to these roles (admins always pass). */
  roles?: Role[];
}) {
  const { user, loading } = useAuth();
  const router = useRouter();
  const pathname = usePathname();
  const allowed = !!user && (!roles || user.role === "admin" || roles.includes(user.role));

  useEffect(() => {
    if (loading) return;
    if (!user) router.replace(`/login?next=${encodeURIComponent(pathname)}`);
    else if (!allowed) router.replace("/");
  }, [loading, user, allowed, router, pathname]);

  if (loading || !allowed) return <Loading />;
  return <>{children}</>;
}

/** For login/register: signed-in users are sent on to where they were going. */
export function RedirectIfAuthenticated({ children }: { children: React.ReactNode }) {
  const { user, loading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (!loading && user) router.replace(safeNext());
  }, [loading, user, router]);

  if (loading || user) return <Loading />;
  return <>{children}</>;
}

/** The `?next=` target, restricted to same-site paths (no open redirects). */
export function safeNext(): string {
  if (typeof window === "undefined") return "/";
  const next = new URLSearchParams(window.location.search).get("next");
  return next && next.startsWith("/") && !next.startsWith("//") ? next : "/";
}
