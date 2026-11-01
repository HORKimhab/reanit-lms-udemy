"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { useState } from "react";
import { CurriculumEditor } from "@/components/instructor/curriculum-editor";
import { LandingForm } from "@/components/instructor/landing-form";
import { PricingForm } from "@/components/instructor/pricing-form";
import { StatusBadge } from "@/components/instructor/status-badge";
import { SubmitPanel } from "@/components/instructor/submit-panel";
import { useCourse } from "@/components/instructor/use-course";
import { cn } from "@/lib/utils";

const TABS = [
  { id: "landing", label: "Landing page" },
  { id: "curriculum", label: "Curriculum" },
  { id: "pricing", label: "Pricing" },
] as const;
type Tab = (typeof TABS)[number]["id"];

export default function CourseEditorPage() {
  const { id } = useParams<{ id: string }>();
  const course = useCourse(id);
  const [tab, setTab] = useState<Tab>("landing");

  if (course.isLoading) return <p className="p-10 text-muted-foreground">Loading…</p>;
  if (!course.data) return <p className="p-10 text-muted-foreground">Course not found.</p>;

  const c = course.data;
  const locked = c.status === "pending";

  return (
    <main className="mx-auto w-full max-w-5xl px-4 py-8">
      <Link href="/instructor/courses" className="text-sm text-muted-foreground hover:text-foreground">
        ← All courses
      </Link>
      <div className="mt-2 mb-6 flex flex-wrap items-center gap-3">
        <h1 className="text-2xl font-bold">{c.title}</h1>
        <StatusBadge status={c.status} />
      </div>

      <div className="grid gap-8 md:grid-cols-[1fr_260px]">
        <div>
          <nav className="mb-6 flex gap-1 border-b">
            {TABS.map((t) => (
              <button
                key={t.id}
                onClick={() => setTab(t.id)}
                className={cn(
                  "-mb-px border-b-2 px-3 py-2 text-sm font-medium",
                  tab === t.id ? "border-primary" : "border-transparent text-muted-foreground hover:text-foreground",
                )}
              >
                {t.label}
              </button>
            ))}
          </nav>
          {tab === "landing" && <LandingForm key={c.updated_at} course={c} locked={locked} />}
          {tab === "curriculum" && <CurriculumEditor courseId={c.id} locked={locked} />}
          {tab === "pricing" && <PricingForm key={c.updated_at} course={c} locked={locked} />}
        </div>
        <aside className="space-y-3">
          <h2 className="font-semibold">Publish</h2>
          <SubmitPanel course={c} />
        </aside>
      </div>
    </main>
  );
}
