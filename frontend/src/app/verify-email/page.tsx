"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, useEffect, useRef, useState } from "react";
import { AuthCard } from "@/components/auth/auth-card";
import { buttonVariants } from "@/components/ui/button";
import { post, type User } from "@/lib/api";
import { useAuth } from "@/lib/auth";

type Status = "verifying" | "verified" | "failed";

function VerifyEmail() {
  const token = useSearchParams().get("token");
  const { user, setUser } = useAuth();
  const [status, setStatus] = useState<Status>(token ? "verifying" : "failed");
  const [verified, setVerified] = useState<User | null>(null);
  const started = useRef(false);

  useEffect(() => {
    // Tokens are single-use: guard against React running the effect twice.
    if (!token || started.current) return;
    started.current = true;
    post<User>("/auth/verify-email", { token })
      .then((u) => {
        setVerified(u);
        setStatus("verified");
      })
      .catch(() => setStatus("failed"));
  }, [token]);

  // The session may finish restoring after verification; sync it whenever both exist.
  useEffect(() => {
    if (verified && user?.id === verified.id && !user.email_verified) setUser(verified);
  }, [verified, user, setUser]);

  if (status === "verifying") return <AuthCard title="Verifying your email…" />;
  if (status === "verified")
    return (
      <AuthCard title="Email verified" description="Thanks! Your email address is confirmed.">
        <Link href="/" className={buttonVariants({ className: "w-full" })}>
          Start learning
        </Link>
      </AuthCard>
    );
  return (
    <AuthCard
      title="Link invalid or expired"
      description="Log in and request a new verification email from the banner at the top of the page."
    >
      <Link href="/login" className={buttonVariants({ variant: "outline", className: "w-full" })}>
        Log in
      </Link>
    </AuthCard>
  );
}

export default function VerifyEmailPage() {
  return (
    <Suspense fallback={<AuthCard title="Verifying your email…" />}>
      <VerifyEmail />
    </Suspense>
  );
}
