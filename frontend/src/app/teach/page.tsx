"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { AuthCard, FormError } from "@/components/auth/auth-card";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { ApiError, post, type User } from "@/lib/api";
import { useAuth } from "@/lib/auth";

export default function TeachPage() {
  const { user, loading, setUser } = useAuth();
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  if (loading) return <AuthCard title="Loading…" />;

  if (!user)
    return (
      <AuthCard title="Teach on ReanIT" description="Share your knowledge with students across Cambodia and beyond.">
        <Link href="/register?next=/teach" className={buttonVariants({ className: "w-full" })}>
          Create an account to get started
        </Link>
      </AuthCard>
    );

  if (user.role !== "student")
    return (
      <AuthCard title="You're an instructor" description="Head to your dashboard to manage your courses.">
        <Link href="/instructor/courses" className={buttonVariants({ className: "w-full" })}>
          Go to instructor dashboard
        </Link>
      </AuthCard>
    );

  if (!user.email_verified)
    return (
      <AuthCard
        title="Verify your email first"
        description="Use the link we emailed you (or resend it from the banner above), then come back here."
      />
    );

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = new FormData(e.currentTarget);
    setPending(true);
    setError(null);
    try {
      const updated = await post<User>("/me/become-instructor", {
        headline: String(form.get("headline")),
        bio: String(form.get("bio")),
        website: String(form.get("website")) || null,
      });
      setUser(updated);
      router.push("/instructor/courses");
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Please try again.");
    } finally {
      setPending(false);
    }
  }

  return (
    <AuthCard title="Become an instructor" description="Tell students who you are.">
      <form onSubmit={onSubmit} className="space-y-4">
        <FormError message={error} />
        <div className="space-y-2">
          <Label htmlFor="headline">Headline</Label>
          <Input id="headline" name="headline" required maxLength={60} placeholder="Senior Rust engineer" />
        </div>
        <div className="space-y-2">
          <Label htmlFor="bio">Bio</Label>
          <Textarea id="bio" name="bio" rows={4} maxLength={5000} />
        </div>
        <div className="space-y-2">
          <Label htmlFor="website">Website (optional)</Label>
          <Input id="website" name="website" type="url" placeholder="https://" />
        </div>
        <Button type="submit" className="w-full" disabled={pending}>
          {pending ? "Saving…" : "Start teaching"}
        </Button>
      </form>
    </AuthCard>
  );
}
