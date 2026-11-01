"use client";

import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect } from "@/components/ui/native-select";
import { Textarea } from "@/components/ui/textarea";
import { api, ApiError, type Category, type Course, type Level } from "@/lib/api";
import { ThumbnailUpload } from "./thumbnail-upload";
import { useUpdateCourse } from "./use-course";

const lines = (text: string) => text.split("\n").map((l) => l.trim()).filter(Boolean);

export function LandingForm({ course, locked }: { course: Course; locked: boolean }) {
  const update = useUpdateCourse(course.id);
  const categories = useQuery({ queryKey: ["categories"], queryFn: () => api<Category[]>("/categories") });
  const [saved, setSaved] = useState(false);

  return (
    <div className="space-y-8">
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          setSaved(false);
          update.mutate(
            {
              title: String(f.get("title")),
              subtitle: String(f.get("subtitle")),
              description: String(f.get("description")),
              category_id: String(f.get("category_id")) || undefined,
              level: String(f.get("level")) as Level,
              language: String(f.get("language")),
              outcomes: lines(String(f.get("outcomes"))),
              requirements: lines(String(f.get("requirements"))),
            },
            { onSuccess: () => setSaved(true) },
          );
        }}
      >
        <fieldset disabled={locked} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="title">Course title</Label>
            <Input id="title" name="title" defaultValue={course.title} required maxLength={60} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="subtitle">Subtitle</Label>
            <Input id="subtitle" name="subtitle" defaultValue={course.subtitle} maxLength={120} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="description">Description</Label>
            <Textarea id="description" name="description" defaultValue={course.description} rows={8} />
          </div>
          <div className="grid gap-4 md:grid-cols-3">
            <div className="space-y-2">
              <Label htmlFor="category_id">Category</Label>
              <NativeSelect id="category_id" name="category_id" defaultValue={course.category_id ?? ""}>
                <option value="" disabled>
                  Choose…
                </option>
                {categories.data?.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </NativeSelect>
            </div>
            <div className="space-y-2">
              <Label htmlFor="level">Level</Label>
              <NativeSelect id="level" name="level" defaultValue={course.level}>
                <option value="all">All levels</option>
                <option value="beginner">Beginner</option>
                <option value="intermediate">Intermediate</option>
                <option value="expert">Expert</option>
              </NativeSelect>
            </div>
            <div className="space-y-2">
              <Label htmlFor="language">Language</Label>
              <NativeSelect id="language" name="language" defaultValue={course.language}>
                <option value="en">English</option>
                <option value="km">Khmer</option>
              </NativeSelect>
            </div>
          </div>
          <div className="grid gap-4 md:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="outcomes">What students will learn (one per line)</Label>
              <Textarea id="outcomes" name="outcomes" defaultValue={course.outcomes.join("\n")} rows={5} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="requirements">Requirements (one per line)</Label>
              <Textarea id="requirements" name="requirements" defaultValue={course.requirements.join("\n")} rows={5} />
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Button type="submit" disabled={update.isPending}>
              {update.isPending ? "Saving…" : "Save"}
            </Button>
            {saved && <span className="text-sm text-muted-foreground">Saved</span>}
          </div>
        </fieldset>
        <FormError message={update.error instanceof ApiError ? update.error.message : update.error ? "Could not save." : null} />
      </form>

      <ThumbnailUpload course={course} locked={locked} />
    </div>
  );
}
