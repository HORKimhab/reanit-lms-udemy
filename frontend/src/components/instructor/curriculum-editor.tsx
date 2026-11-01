"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { Textarea } from "@/components/ui/textarea";
import { ApiError, del, patch, post, put, type Lecture, type LectureKind, type Section } from "@/lib/api";
import { curriculumKey, useCurriculum } from "./use-course";

type Order = { sections: { id: string; lecture_ids: string[] }[] };

function toOrder(sections: Section[]): Order {
  return { sections: sections.map((s) => ({ id: s.id, lecture_ids: s.lectures.map((l) => l.id) })) };
}

function swap<T>(items: T[], i: number, j: number): T[] {
  const next = [...items];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

/** Moves a lecture up/down, crossing into the neighbouring section at the edges. */
function moveLecture(sections: Section[], sIdx: number, lIdx: number, dir: -1 | 1): Section[] | null {
  const next = sections.map((s) => ({ ...s, lectures: [...s.lectures] }));
  const lectures = next[sIdx].lectures;
  const target = lIdx + dir;
  if (target >= 0 && target < lectures.length) {
    next[sIdx].lectures = swap(lectures, lIdx, target);
    return next;
  }
  const neighbour = next[sIdx + dir];
  if (!neighbour) return null;
  const [lecture] = lectures.splice(lIdx, 1);
  if (dir === -1) neighbour.lectures.push(lecture);
  else neighbour.lectures.unshift(lecture);
  return next;
}

export function CurriculumEditor({ courseId, locked }: { courseId: string; locked: boolean }) {
  const queryClient = useQueryClient();
  const curriculum = useCurriculum(courseId);
  const base = `/instructor/courses/${courseId}`;
  const refresh = () => queryClient.invalidateQueries({ queryKey: curriculumKey(courseId) });

  const action = useMutation({ mutationFn: (run: () => Promise<unknown>) => run(), onSuccess: refresh });
  const reorder = useMutation({
    mutationFn: (sections: Section[]) => put<Section[]>(`${base}/curriculum/order`, toOrder(sections)),
    onMutate: (sections) => queryClient.setQueryData(curriculumKey(courseId), sections),
    onSettled: refresh,
  });

  const sections = curriculum.data ?? [];
  const error = action.error ?? reorder.error;

  return (
    <div className="space-y-4">
      <FormError message={error instanceof ApiError ? error.message : error ? "Something went wrong." : null} />
      {curriculum.isLoading && <p className="text-muted-foreground">Loading…</p>}

      {sections.map((section, sIdx) => (
        <Card key={section.id}>
          <CardHeader className="flex flex-row items-center gap-2">
            <span className="text-sm font-semibold text-muted-foreground">Section {sIdx + 1}:</span>
            <InlineTitle
              value={section.title}
              disabled={locked}
              onSave={(title) => action.mutate(() => patch(`${base}/sections/${section.id}`, { title }))}
            />
            <div className="ml-auto flex gap-1">
              <Button size="sm" variant="ghost" disabled={locked || sIdx === 0}
                onClick={() => reorder.mutate(swap(sections, sIdx, sIdx - 1))} aria-label="Move section up">↑</Button>
              <Button size="sm" variant="ghost" disabled={locked || sIdx === sections.length - 1}
                onClick={() => reorder.mutate(swap(sections, sIdx, sIdx + 1))} aria-label="Move section down">↓</Button>
              <Button size="sm" variant="ghost" disabled={locked}
                onClick={() => {
                  if (confirm(`Delete section "${section.title}" and all its lectures?`))
                    action.mutate(() => del(`${base}/sections/${section.id}`));
                }}>
                Delete
              </Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-2">
            {section.lectures.map((lecture, lIdx) => (
              <LectureRow
                key={lecture.id}
                lecture={lecture}
                index={lIdx}
                locked={locked}
                canMoveUp={sIdx > 0 || lIdx > 0}
                canMoveDown={sIdx < sections.length - 1 || lIdx < section.lectures.length - 1}
                onMove={(dir) => {
                  const next = moveLecture(sections, sIdx, lIdx, dir);
                  if (next) reorder.mutate(next);
                }}
                onSave={(body) => action.mutate(() => patch(`${base}/lectures/${lecture.id}`, body))}
                onDelete={() => {
                  if (confirm(`Delete lecture "${lecture.title}"?`))
                    action.mutate(() => del(`${base}/lectures/${lecture.id}`));
                }}
              />
            ))}
            {!locked && (
              <AddLecture onAdd={(title, kind) => action.mutate(() => post(`${base}/sections/${section.id}/lectures`, { title, kind }))} />
            )}
          </CardContent>
        </Card>
      ))}

      {!locked && <AddSection onAdd={(title) => action.mutate(() => post(`${base}/sections`, { title }))} />}
    </div>
  );
}

function InlineTitle({ value, disabled, onSave }: { value: string; disabled: boolean; onSave: (v: string) => void }) {
  const [editing, setEditing] = useState(false);
  if (!editing)
    return (
      <button className="text-left font-medium hover:underline disabled:no-underline" disabled={disabled} onClick={() => setEditing(true)}>
        {value}
      </button>
    );
  return (
    <form
      className="flex gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(String(new FormData(e.currentTarget).get("title")));
        setEditing(false);
      }}
    >
      <Input name="title" defaultValue={value} autoFocus maxLength={80} />
      <Button size="sm" type="submit">Save</Button>
      <Button size="sm" variant="ghost" type="button" onClick={() => setEditing(false)}>Cancel</Button>
    </form>
  );
}

function LectureRow(props: {
  lecture: Lecture;
  index: number;
  locked: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMove: (dir: -1 | 1) => void;
  onSave: (body: Partial<Lecture>) => void;
  onDelete: () => void;
}) {
  const { lecture, locked } = props;
  const [open, setOpen] = useState(false);
  const missing = lecture.kind === "video" ? !lecture.video_asset_id : !lecture.article_body.trim();

  return (
    <div className="rounded-lg border">
      <div className="flex items-center gap-2 px-3 py-2">
        <span className="text-sm text-muted-foreground">{props.index + 1}.</span>
        <span aria-hidden>{lecture.kind === "video" ? "▶" : "📄"}</span>
        <InlineTitle value={lecture.title} disabled={locked} onSave={(title) => props.onSave({ title })} />
        {lecture.is_preview && <Badge variant="outline">Preview</Badge>}
        {missing && <Badge variant="destructive">{lecture.kind === "video" ? "No video" : "Empty"}</Badge>}
        <div className="ml-auto flex gap-1">
          <Button size="sm" variant="ghost" onClick={() => setOpen((v) => !v)}>{open ? "Close" : "Edit"}</Button>
          <Button size="sm" variant="ghost" disabled={locked || !props.canMoveUp} onClick={() => props.onMove(-1)} aria-label="Move lecture up">↑</Button>
          <Button size="sm" variant="ghost" disabled={locked || !props.canMoveDown} onClick={() => props.onMove(1)} aria-label="Move lecture down">↓</Button>
          <Button size="sm" variant="ghost" disabled={locked} onClick={props.onDelete}>Delete</Button>
        </div>
      </div>
      {open && (
        <div className="space-y-3 border-t px-3 py-3">
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={lecture.is_preview} disabled={locked}
              onChange={(e) => props.onSave({ is_preview: e.target.checked })} />
            Free preview (anyone can watch without enrolling)
          </label>
          {lecture.kind === "article" ? (
            <form className="space-y-2" onSubmit={(e) => {
              e.preventDefault();
              props.onSave({ article_body: String(new FormData(e.currentTarget).get("body")) });
            }}>
              <Textarea name="body" defaultValue={lecture.article_body} rows={10} disabled={locked} placeholder="Write the lecture (Markdown supported)" />
              <Button size="sm" type="submit" disabled={locked}>Save article</Button>
            </form>
          ) : (
            <p className="text-sm text-muted-foreground">Video upload is coming in the next milestone.</p>
          )}
        </div>
      )}
    </div>
  );
}

function AddLecture({ onAdd }: { onAdd: (title: string, kind: LectureKind) => void }) {
  return (
    <form className="flex gap-2 pt-2" onSubmit={(e) => {
      e.preventDefault();
      const f = new FormData(e.currentTarget);
      onAdd(String(f.get("title")), String(f.get("kind")) as LectureKind);
      e.currentTarget.reset();
    }}>
      <Input name="title" placeholder="New lecture title" required maxLength={80} />
      <NativeSelect name="kind" className="w-32" defaultValue="video">
        <option value="video">Video</option>
        <option value="article">Article</option>
      </NativeSelect>
      <Button type="submit" variant="outline">Add lecture</Button>
    </form>
  );
}

function AddSection({ onAdd }: { onAdd: (title: string) => void }) {
  return (
    <form className="flex gap-2" onSubmit={(e) => {
      e.preventDefault();
      onAdd(String(new FormData(e.currentTarget).get("title")));
      e.currentTarget.reset();
    }}>
      <Input name="title" placeholder="New section title" required maxLength={80} />
      <Button type="submit">Add section</Button>
    </form>
  );
}
