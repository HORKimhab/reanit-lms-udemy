"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { FormError } from "@/components/auth/auth-card";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ApiError, api, formatPrice, post, type Course } from "@/lib/api";

type PendingCourse = Course & { instructor_name: string };

export default function AdminCoursesPage() {
  const queryClient = useQueryClient();
  const pending = useQuery({
    queryKey: ["admin-courses", "pending"],
    queryFn: () => api<PendingCourse[]>("/admin/courses?status=pending"),
  });
  const review = useMutation({
    mutationFn: ({ id, action, reason }: { id: string; action: "approve" | "reject"; reason?: string }) =>
      post<Course>(`/admin/courses/${id}/${action}`, action === "reject" ? { reason } : undefined),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["admin-courses"] }),
  });

  return (
    <main className="mx-auto w-full max-w-5xl px-4 py-10">
      <h1 className="mb-6 text-3xl font-bold">Courses awaiting review</h1>
      <FormError message={review.error instanceof ApiError ? review.error.message : null} />
      {pending.isLoading && <p className="text-muted-foreground">Loading…</p>}
      {pending.data?.length === 0 && <p className="text-muted-foreground">Nothing to review. 🎉</p>}
      <div className="space-y-3">
        {pending.data?.map((c) => (
          <Card key={c.id}>
            <CardContent className="flex flex-col gap-4 md:flex-row md:items-center">
              <div className="min-w-0 flex-1">
                <p className="font-semibold">{c.title}</p>
                <p className="text-sm text-muted-foreground">
                  by {c.instructor_name} · {formatPrice(c.price_minor, c.currency)} · submitted{" "}
                  {new Date(c.updated_at).toLocaleDateString()}
                </p>
              </div>
              <div className="flex gap-2">
                <Link href={`/instructor/courses/${c.id}`} className={buttonVariants({ variant: "outline" })}>
                  Inspect
                </Link>
                <Button
                  disabled={review.isPending}
                  onClick={() => review.mutate({ id: c.id, action: "approve" })}
                >
                  Approve
                </Button>
                <Button
                  variant="destructive"
                  disabled={review.isPending}
                  onClick={() => {
                    const reason = prompt("What should the instructor change?");
                    if (reason?.trim()) review.mutate({ id: c.id, action: "reject", reason });
                  }}
                >
                  Request changes
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </main>
  );
}
