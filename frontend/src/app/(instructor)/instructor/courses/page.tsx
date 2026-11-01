"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { StatusBadge } from "@/components/instructor/status-badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { api, ApiError, formatPrice, post, type Category, type Course } from "@/lib/api";

export default function InstructorCoursesPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [creating, setCreating] = useState(false);
  const courses = useQuery({ queryKey: ["instructor-courses"], queryFn: () => api<Course[]>("/instructor/courses") });
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });

  const create = useMutation({
    mutationFn: (body: { title: string; category_id: string | null }) =>
      post<Course>("/instructor/courses", body),
    onSuccess: (course) => {
      queryClient.invalidateQueries({ queryKey: ["instructor-courses"] });
      router.push(`/instructor/courses/${course.id}`);
    },
  });

  return (
    <main className="mx-auto w-full max-w-5xl px-4 py-10">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="text-3xl font-bold">Courses</h1>
        <Button onClick={() => setCreating((v) => !v)}>{creating ? "Cancel" : "New course"}</Button>
      </div>

      {creating && (
        <Card className="mb-6">
          <CardContent>
            <form
              className="flex flex-col gap-3 md:flex-row"
              onSubmit={(e) => {
                e.preventDefault();
                const form = new FormData(e.currentTarget);
                create.mutate({
                  title: String(form.get("title")),
                  category_id: String(form.get("category_id")) || null,
                });
              }}
            >
              <Input name="title" placeholder="Working title, e.g. Rust for Web Developers" required maxLength={60} />
              <NativeSelect name="category_id" className="md:w-56" defaultValue="">
                <option value="">Choose a category later</option>
                {categories.data?.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </NativeSelect>
              <Button type="submit" disabled={create.isPending}>
                Create
              </Button>
            </form>
            <div className="mt-3">
              <FormError message={create.error instanceof ApiError ? create.error.message : create.error ? "Could not create course." : null} />
            </div>
          </CardContent>
        </Card>
      )}

      {courses.isLoading && <p className="text-muted-foreground">Loading…</p>}
      {courses.data?.length === 0 && (
        <p className="text-muted-foreground">You have no courses yet. Create your first one!</p>
      )}
      <div className="space-y-3">
        {courses.data?.map((course) => (
          <Link key={course.id} href={`/instructor/courses/${course.id}`} className="block">
            <Card className="transition hover:shadow-md">
              <CardContent className="flex items-center gap-4">
                <div className="aspect-video w-32 shrink-0 overflow-hidden rounded-md bg-muted">
                  {course.thumbnail_url && (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img src={course.thumbnail_url} alt="" className="size-full object-cover" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate font-semibold">{course.title}</p>
                  <p className="text-sm text-muted-foreground">{formatPrice(course.price_minor, course.currency)}</p>
                </div>
                <StatusBadge status={course.status} />
              </CardContent>
            </Card>
          </Link>
        ))}
      </div>
    </main>
  );
}
