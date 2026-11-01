"use client";

import { useQuery } from "@tanstack/react-query";
import Link from "next/link";
import { CourseGrid } from "@/components/catalog/course-card";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { api, type Category, type CourseCard, type Page } from "@/lib/api";

export default function Home() {
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const newest = useQuery({
    queryKey: ["courses", "newest"],
    queryFn: () => api<Page<CourseCard>>("/courses?sort=newest&per_page=8"),
  });

  return (
    <main>
      <section className="border-b bg-muted/40">
        <div className="mx-auto max-w-7xl px-4 py-20">
          <h1 className="max-w-2xl text-4xl font-bold tracking-tight md:text-5xl">
            Learn skills that move your career forward
          </h1>
          <p className="mt-4 max-w-xl text-lg text-muted-foreground">
            Courses from expert instructors. Pay easily with Bakong KHQR or ABA, learn at your own pace.
          </p>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-4 py-12">
        <div className="mb-6 flex items-end justify-between">
          <h2 className="text-2xl font-semibold">New courses</h2>
          <Link href="/courses" className="text-sm font-medium underline">
            Browse all
          </Link>
        </div>
        {newest.isLoading && <p className="text-muted-foreground">Loading…</p>}
        {newest.isError && <p className="text-muted-foreground">Courses are unavailable right now.</p>}
        {newest.data?.data.length === 0 && <p className="text-muted-foreground">No courses published yet.</p>}
        {newest.data && <CourseGrid courses={newest.data.data} />}
      </section>

      <section className="mx-auto max-w-7xl px-4 pb-16">
        <h2 className="mb-6 text-2xl font-semibold">Top categories</h2>
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {categories.data?.map((c) => (
            <Link key={c.id} href={`/courses?category=${c.slug}`}>
              <Card className="transition hover:shadow-md">
                <CardHeader>
                  <CardTitle className="text-base">{c.name}</CardTitle>
                </CardHeader>
                <CardContent className="text-sm text-muted-foreground">Explore courses</CardContent>
              </Card>
            </Link>
          ))}
        </div>
      </section>
    </main>
  );
}
