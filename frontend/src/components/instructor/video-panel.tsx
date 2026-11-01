"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { HlsPlayer } from "@/components/video/hls-player";
import { API_URL, api, formatDuration, post, type Lecture, type VideoInfo } from "@/lib/api";
import { curriculumKey } from "./use-course";

type UploadUrl = { asset_id: string; upload_url: string; expires_in_secs: number };

/** PUT with progress events (fetch can't report upload progress). */
function putWithProgress(url: string, file: File, onProgress: (fraction: number) => void): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("PUT", url);
    xhr.setRequestHeader("Content-Type", file.type);
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onProgress(e.loaded / e.total);
    };
    xhr.onload = () => (xhr.status < 300 ? resolve() : reject(new Error(`Upload failed (${xhr.status})`)));
    xhr.onerror = () => reject(new Error("Upload failed: network error"));
    xhr.send(file);
  });
}

export function VideoPanel({ courseId, lecture, locked }: { courseId: string; lecture: Lecture; locked: boolean }) {
  const queryClient = useQueryClient();
  const base = `/instructor/courses/${courseId}/lectures/${lecture.id}/video`;
  const [progress, setProgress] = useState<number | null>(null);

  const video = useQuery({
    queryKey: ["lecture-video", lecture.id, lecture.video_asset_id],
    queryFn: () => api<VideoInfo>(base),
    enabled: !!lecture.video_asset_id,
    refetchInterval: (q) => (q.state.data?.status === "processing" ? 5000 : false),
  });

  const upload = useMutation({
    mutationFn: async (file: File) => {
      const { asset_id, upload_url } = await post<UploadUrl>(`${base}/upload-url`, { content_type: file.type });
      setProgress(0);
      await putWithProgress(upload_url, file, setProgress);
      return post<VideoInfo>(base, { asset_id });
    },
    onSettled: () => {
      setProgress(null);
      queryClient.invalidateQueries({ queryKey: curriculumKey(courseId) });
    },
  });

  const info = video.data;

  return (
    <div className="space-y-3">
      {info?.status === "ready" && (
        <>
          <HlsPlayer src={`${API_URL}/videos/${info.asset_id}/playlist/master.m3u8`} />
          <p className="text-sm text-muted-foreground">Duration {formatDuration(info.duration_sec)}</p>
        </>
      )}
      {info?.status === "processing" && (
        <p className="text-sm text-muted-foreground">Processing your video… this can take a few minutes.</p>
      )}
      {info?.status === "failed" && (
        <p className="text-sm text-destructive">Processing failed: {info.error}. Please upload the file again.</p>
      )}

      {progress !== null && (
        <div className="space-y-1">
          <div className="h-2 overflow-hidden rounded-full bg-muted">
            <div className="h-full bg-primary transition-all" style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
          <p className="text-xs text-muted-foreground">Uploading… {Math.round(progress * 100)}%</p>
        </div>
      )}

      <label className="block text-sm">
        <span className="mb-1 block font-medium">{lecture.video_asset_id ? "Replace video" : "Upload video"}</span>
        <input
          type="file"
          accept="video/mp4,video/quicktime,video/webm,video/x-matroska"
          disabled={locked || upload.isPending}
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) upload.mutate(file);
            e.target.value = "";
          }}
        />
      </label>
      <p className="text-xs text-muted-foreground">MP4, MOV, WebM or MKV. We convert it to 360p and 720p streams.</p>
      <FormError message={upload.error ? upload.error.message : null} />
    </div>
  );
}
