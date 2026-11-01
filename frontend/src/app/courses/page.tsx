"use client";

import { useQuery } from "@tanstack/react-query";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { Suspense } from "react";
import { CourseGrid } from "@/components/catalog/course-card";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { api, LEVEL_LABELS, type Category, type CourseCard, type Level, type Page } from "@/lib/api";

const PER_PAGE = 20;

function SearchPage() {
  const params = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });

  const query = new URLSearchParams(params);
  query.set("per_page", String(PER_PAGE));
  const results = useQuery({
    queryKey: ["courses", query.toString()],
    queryFn: () => api<Page<CourseCard>>(`/courses?${query}`),
  });

  /** Updates one filter in the URL (resetting to page 1). */
  function setParam(key: string, value: string | null) {
    const next = new URLSearchParams(params);
    if (value) next.set(key, value);
    else next.delete(key);
    if (key !== "page") next.delete("page");
    router.push(`${pathname}?${next}`);
  }

  const q = params.get("q");
  const page = Number(params.get("page") ?? 1);
  const meta = results.data?.meta;
  const pages = meta ? Math.max(1, Math.ceil(meta.total / meta.per_page)) : 1;

  return (
    <main className="mx-auto w-full max-w-7xl px-4 py-8">
      <h1 className="mb-6 text-2xl font-bold">
        {q ? `Results for “${q}”` : "All courses"}
        {meta && <span className="ml-2 text-base font-normal text-muted-foreground">({meta.total})</span>}
      </h1>
      <div className="grid gap-8 md:grid-cols-[220px_1fr]">
        <aside className="space-y-4">
          <Filter label="Sort by" value={params.get("sort") ?? ""} onChange={(v) => setParam("sort", v)}>
            <option value="">{q ? "Most relevant" : "Newest"}</option>
            <option value="newest">Newest</option>
            <option value="price_asc">Price: low to high</option>
            <option value="price_desc">Price: high to low</option>
          </Filter>
          <Filter label="Category" value={params.get("category") ?? ""} onChange={(v) => setParam("category", v)}>
            <option value="">All categories</option>
            {categories.data?.map((c) => (
              <option key={c.id} value={c.slug}>
                {c.name}
              </option>
            ))}
          </Filter>
          <Filter label="Level" value={params.get("level") ?? ""} onChange={(v) => setParam("level", v)}>
            <option value="">Any level</option>
            {(Object.keys(LEVEL_LABELS) as Level[]).map((l) => (
              <option key={l} value={l}>
                {LEVEL_LABELS[l]}
              </option>
            ))}
          </Filter>
          <Filter label="Price" value={params.get("price") ?? ""} onChange={(v) => setParam("price", v)}>
            <option value="">Any price</option>
            <option value="free">Free</option>
            <option value="paid">Paid</option>
          </Filter>
        </aside>

        <section>
          {results.isLoading && <p className="text-muted-foreground">Loading…</p>}
          {results.isError && <p className="text-muted-foreground">Search is unavailable right now.</p>}
          {results.data?.data.length === 0 && (
            <p className="text-muted-foreground">No courses match. Try different words or fewer filters.</p>
          )}
          {results.data && <CourseGrid courses={results.data.data} />}
          {pages > 1 && (
            <div className="mt-8 flex items-center justify-center gap-3">
              <Button variant="outline" disabled={page <= 1} onClick={() => setParam("page", String(page - 1))}>
                Previous
              </Button>
              <span className="text-sm text-muted-foreground">
                Page {page} of {pages}
              </span>
              <Button variant="outline" disabled={page >= pages} onClick={() => setParam("page", String(page + 1))}>
                Next
              </Button>
            </div>
          )}
        </section>
      </div>
    </main>
  );
}

function Filter(props: { label: string; value: string; onChange: (v: string | null) => void; children: React.ReactNode }) {
  return (
    <label className="block space-y-1 text-sm">
      <span className="font-medium">{props.label}</span>
      <NativeSelect value={props.value} onChange={(e) => props.onChange(e.target.value || null)}>
        {props.children}
      </NativeSelect>
    </label>
  );
}

export default function CoursesPage() {
  return (
    <Suspense>
      <SearchPage />
    </Suspense>
  );
}
