"use client";

import { useQuery } from "@tanstack/react-query";
import { useParams } from "next/navigation";
import { Suspense, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { HlsPlayer } from "@/components/video/hls-player";
import {
  API_URL,
  ApiError,
  api,
  formatDuration,
  formatPrice,
  LANGUAGE_LABELS,
  LEVEL_LABELS,
  type CoursePage,
  type PublicLecture,
} from "@/lib/api";

function CourseLanding() {
  const { slug } = useParams<{ slug: string }>();
  const course = useQuery({
    queryKey: ["course-page", slug],
    queryFn: () => api<CoursePage>(`/courses/${slug}`),
    retry: (count, err) => !(err instanceof ApiError && err.status === 404) && count < 2,
  });
  const [preview, setPreview] = useState<PublicLecture | null>(null);

  if (course.isLoading) return <p className="p-10 text-muted-foreground">Loading…</p>;
  if (!course.data) return <p className="p-10 text-muted-foreground">This course doesn&apos;t exist or isn&apos;t available.</p>;
  const c = course.data;
  const firstPreview = c.curriculum.flatMap((s) => s.lectures).find((l) => l.is_preview);

  return (
    <main>
      <section className="bg-zinc-900 text-zinc-50">
        <div className="mx-auto grid max-w-6xl gap-8 px-4 py-10 md:grid-cols-[1fr_320px]">
          <div className="space-y-3">
            <h1 className="text-3xl font-bold">{c.title}</h1>
            <p className="text-lg text-zinc-300">{c.subtitle}</p>
            <p className="text-sm text-zinc-300">
              Created by <span className="font-medium text-zinc-50">{c.instructor.name}</span>
            </p>
            <p className="text-sm text-zinc-400">
              {LEVEL_LABELS[c.level]} · {LANGUAGE_LABELS[c.language] ?? c.language} · {c.lecture_count} lectures ·{" "}
              {formatDuration(c.duration_sec)} total
            </p>
          </div>
          <Card className="self-start text-foreground">
            {c.thumbnail_url && (
              // eslint-disable-next-line @next/next/no-img-element
              <img src={c.thumbnail_url} alt="" className="aspect-video w-full rounded-t-xl object-cover" />
            )}
            <CardContent className="space-y-3">
              <p className="text-3xl font-bold">{formatPrice(c.price_minor, c.currency)}</p>
              <Button className="w-full" disabled title="Enrollment opens soon">
                {c.price_minor === 0 ? "Enroll for free" : "Buy now"} (coming soon)
              </Button>
              {firstPreview && (
                <Button variant="outline" className="w-full" onClick={() => setPreview(firstPreview)}>
                  Preview this course
                </Button>
              )}
            </CardContent>
          </Card>
        </div>
      </section>

      <div className="mx-auto max-w-6xl space-y-10 px-4 py-10 md:pr-[352px]">
        {c.outcomes.length > 0 && (
          <section className="rounded-lg border p-6">
            <h2 className="mb-4 text-xl font-bold">What you&apos;ll learn</h2>
            <ul className="grid gap-2 text-sm sm:grid-cols-2">
              {c.outcomes.map((o) => (
                <li key={o}>✓ {o}</li>
              ))}
            </ul>
          </section>
        )}

        <section>
          <h2 className="mb-4 text-xl font-bold">Course content</h2>
          <div className="divide-y rounded-lg border">
            {c.curriculum.map((s) => (
              <details key={s.id} className="group" open={c.curriculum.length <= 3}>
                <summary className="flex cursor-pointer justify-between bg-muted/50 px-4 py-3 font-medium">
                  {s.title}
                  <span className="text-sm font-normal text-muted-foreground">{s.lectures.length} lectures</span>
                </summary>
                <ul>
                  {s.lectures.map((l) => (
                    <li key={l.id} className="flex items-center gap-3 px-4 py-2 text-sm">
                      <span aria-hidden>{l.kind === "video" ? "▶" : "📄"}</span>
                      {l.is_preview ? (
                        <button className="text-left underline" onClick={() => setPreview(l)}>
                          {l.title}
                        </button>
                      ) : (
                        <span>{l.title}</span>
                      )}
                      {l.is_preview && <span className="text-xs text-primary">Preview</span>}
                      {l.duration_sec > 0 && (
                        <span className="ml-auto text-muted-foreground">{formatDuration(l.duration_sec)}</span>
                      )}
                    </li>
                  ))}
                </ul>
              </details>
            ))}
          </div>
        </section>

        {c.requirements.length > 0 && (
          <section>
            <h2 className="mb-4 text-xl font-bold">Requirements</h2>
            <ul className="list-disc space-y-1 pl-5 text-sm">
              {c.requirements.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          </section>
        )}

        <section>
          <h2 className="mb-4 text-xl font-bold">Description</h2>
          <p className="whitespace-pre-line text-sm leading-relaxed">{c.description}</p>
        </section>

        <section>
          <h2 className="mb-2 text-xl font-bold">Instructor</h2>
          <p className="font-semibold">{c.instructor.name}</p>
          <p className="text-sm text-muted-foreground">{c.instructor.headline}</p>
          {c.instructor.bio && <p className="mt-2 whitespace-pre-line text-sm">{c.instructor.bio}</p>}
        </section>
      </div>

      {preview && <PreviewDialog lecture={preview} onClose={() => setPreview(null)} />}
    </main>
  );
}

function PreviewDialog({ lecture, onClose }: { lecture: PublicLecture; onClose: () => void }) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Preview: ${lecture.title}`}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
      onClick={onClose}
    >
      <div className="w-full max-w-3xl rounded-xl bg-background p-4" onClick={(e) => e.stopPropagation()}>
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold">{lecture.title}</h2>
          <Button variant="ghost" size="sm" onClick={onClose}>
            Close
          </Button>
        </div>
        {lecture.kind === "video" && lecture.preview_asset_id ? (
          <HlsPlayer src={`${API_URL}/videos/${lecture.preview_asset_id}/playlist/master.m3u8`} />
        ) : (
          <div className="max-h-[70vh] overflow-y-auto whitespace-pre-line text-sm">{lecture.article_body}</div>
        )}
      </div>
    </div>
  );
}

export default function CourseLandingPage() {
  return (
    <Suspense>
      <CourseLanding />
    </Suspense>
  );
}
