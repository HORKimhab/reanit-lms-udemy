"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { ApiError, post, type Course } from "@/lib/api";
import { courseKey } from "./use-course";

const PREFIX = "course is not ready for review: ";

export function SubmitPanel({ course }: { course: Course }) {
  const queryClient = useQueryClient();
  const submit = useMutation({
    mutationFn: () => post<Course>(`/instructor/courses/${course.id}/submit`),
    onSuccess: (updated) => {
      queryClient.setQueryData(courseKey(course.id), updated);
      queryClient.invalidateQueries({ queryKey: ["instructor-courses"] });
    },
  });

  if (course.status === "pending")
    return <p className="text-sm text-muted-foreground">Your course is being reviewed. Editing is locked until the review is finished.</p>;
  if (course.status === "published")
    return <p className="text-sm text-muted-foreground">Your course is live.</p>;

  const message = submit.error instanceof ApiError ? submit.error.message : null;
  const problems = message?.startsWith(PREFIX) ? message.slice(PREFIX.length).split("; ") : null;

  return (
    <div className="space-y-3">
      {course.status === "rejected" && course.rejection_reason && (
        <p className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Reviewer feedback: {course.rejection_reason}
        </p>
      )}
      <Button onClick={() => submit.mutate()} disabled={submit.isPending}>
        {submit.isPending ? "Submitting…" : "Submit for review"}
      </Button>
      {problems ? (
        <div className="text-sm">
          <p className="font-medium">Before submitting, please:</p>
          <ul className="mt-1 list-disc pl-5 text-muted-foreground">
            {problems.map((p) => <li key={p}>{p}</li>)}
          </ul>
        </div>
      ) : (
        message && <p className="text-sm text-destructive">{message}</p>
      )}
    </div>
  );
}
