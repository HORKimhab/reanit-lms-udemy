"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { Button, buttonVariants } from "@/components/ui/button";
import { useAuth } from "@/lib/auth";

export function UserMenu() {
  const { user, loading, logout } = useAuth();
  const router = useRouter();

  if (loading) return <div className="h-8 w-40" aria-hidden />;

  if (!user)
    return (
      <>
        <Link href="/login" className={buttonVariants({ variant: "outline" })}>
          Log in
        </Link>
        <Link href="/register" className={buttonVariants()}>
          Sign up
        </Link>
      </>
    );

  const initials = user.name
    .split(/\s+/)
    .map((p) => p[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();

  return (
    <>
      {user.role === "admin" && (
        <Link href="/admin/courses" className="hidden text-sm text-muted-foreground hover:text-foreground md:block">
          Admin
        </Link>
      )}
      <Link href="/my-learning" className="hidden text-sm text-muted-foreground hover:text-foreground md:block">
        My learning
      </Link>
      <div className="flex items-center gap-2">
        <span
          title={`${user.name} (${user.role})`}
          className="flex size-8 items-center justify-center rounded-full bg-primary text-xs font-semibold text-primary-foreground"
        >
          {initials}
        </span>
        <Button
          variant="ghost"
          onClick={async () => {
            await logout();
            router.push("/");
          }}
        >
          Log out
        </Button>
      </div>
    </>
  );
}

/** "Teach on ReanIT" for students and visitors; the dashboard link for instructors. */
export function TeachLink() {
  const { user } = useAuth();
  const isInstructor = user?.role === "instructor" || user?.role === "admin";
  return (
    <Link
      href={isInstructor ? "/instructor/courses" : "/teach"}
      className="hidden text-sm text-muted-foreground hover:text-foreground md:block"
    >
      {isInstructor ? "Instructor" : "Teach on ReanIT"}
    </Link>
  );
}
