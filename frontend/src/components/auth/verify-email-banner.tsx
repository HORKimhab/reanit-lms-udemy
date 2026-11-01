"use client";

import { useState } from "react";
import { ApiError, post } from "@/lib/api";
import { useAuth } from "@/lib/auth";

export function VerifyEmailBanner() {
  const { user } = useAuth();
  const [state, setState] = useState<"idle" | "sending" | "sent" | "error" | "wait">("idle");
  if (!user || user.email_verified) return null;

  async function resend() {
    setState("sending");
    try {
      await post("/auth/resend-verification");
      setState("sent");
    } catch (err) {
      setState(err instanceof ApiError && err.status === 429 ? "wait" : "error");
    }
  }

  return (
    <div className="border-b bg-amber-50 px-4 py-2 text-center text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-100">
      Please verify your email ({user.email}) to buy courses and teach.{" "}
      {state === "sent" ? (
        <span className="font-medium">Verification email sent.</span>
      ) : (
        <button
          onClick={resend}
          disabled={state === "sending"}
          className="font-medium underline disabled:opacity-50"
        >
          {state === "error"
            ? "Failed, try again"
            : state === "wait"
              ? "Please wait a minute, then resend"
              : "Resend email"}
        </button>
      )}
    </div>
  );
}
