"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FormError } from "@/components/auth/auth-card";
import { Label } from "@/components/ui/label";
import { post, put, type Course } from "@/lib/api";
import { courseKey } from "./use-course";

type UploadUrl = { upload_url: string; key: string; expires_in_secs: number };

/** Upload straight to object storage with a presigned URL, then attach the key. */
async function uploadThumbnail(courseId: string, file: File): Promise<Course> {
  const { upload_url, key } = await post<UploadUrl>(
    `/instructor/courses/${courseId}/thumbnail/upload-url`,
    { content_type: file.type },
  );
  const res = await fetch(upload_url, { method: "PUT", body: file, headers: { "Content-Type": file.type } });
  if (!res.ok) throw new Error(`Upload failed (${res.status})`);
  return put<Course>(`/instructor/courses/${courseId}/thumbnail`, { key });
}

export function ThumbnailUpload({ course, locked }: { course: Course; locked: boolean }) {
  const queryClient = useQueryClient();
  const upload = useMutation({
    mutationFn: (file: File) => uploadThumbnail(course.id, file),
    onSuccess: (updated) => queryClient.setQueryData(courseKey(course.id), updated),
  });

  return (
    <div className="space-y-2">
      <Label htmlFor="thumbnail">Course image</Label>
      <div className="flex flex-col gap-4 md:flex-row md:items-start">
        <div className="aspect-video w-full max-w-sm overflow-hidden rounded-lg border bg-muted">
          {course.thumbnail_url ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={course.thumbnail_url} alt="Course thumbnail" className="size-full object-cover" />
          ) : (
            <div className="flex size-full items-center justify-center text-sm text-muted-foreground">
              No image yet
            </div>
          )}
        </div>
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>JPEG, PNG or WebP. 1280×720 recommended.</p>
          <input
            id="thumbnail"
            type="file"
            accept="image/jpeg,image/png,image/webp"
            disabled={locked || upload.isPending}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) upload.mutate(file);
              e.target.value = "";
            }}
          />
          {upload.isPending && <p>Uploading…</p>}
          <FormError message={upload.error ? upload.error.message : null} />
        </div>
      </div>
    </div>
  );
}
