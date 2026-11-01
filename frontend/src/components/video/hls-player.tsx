"use client";

import Hls from "hls.js";
import { useEffect, useRef, useState } from "react";
import { API_URL, getAccessToken, refreshSession } from "@/lib/api";

/**
 * Plays an HLS stream served by our API. Playlists need the bearer token;
 * segments are presigned storage URLs and must NOT get an Authorization header
 * (S3 rejects requests that carry both kinds of credentials).
 */
export function HlsPlayer({ src, className }: { src: string; className?: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  // Only rendered client-side (behind auth), so `window` is available here.
  const [supported] = useState(() => typeof window !== "undefined" && Hls.isSupported());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !supported) return;
    const hls = new Hls({
      xhrSetup: (xhr, url) => {
        const token = getAccessToken();
        if (token && url.startsWith(API_URL)) xhr.setRequestHeader("Authorization", `Bearer ${token}`);
      },
    });
    let retriedAuth = false;
    hls.on(Hls.Events.ERROR, async (_event, data) => {
      // An expired access token: refresh once and reload the playlist.
      if (data.response?.code === 401 && !retriedAuth) {
        retriedAuth = true;
        if (await refreshSession()) {
          hls.loadSource(src);
          return;
        }
      }
      if (data.fatal) setError("This video could not be loaded.");
    });
    hls.loadSource(src);
    hls.attachMedia(video);
    return () => hls.destroy();
  }, [src, supported]);

  if (!supported)
    return (
      <p className="text-sm text-destructive">
        Your browser can&apos;t play this video. Please use a recent Chrome, Firefox, Edge or Safari.
      </p>
    );
  if (error) return <p className="text-sm text-destructive">{error}</p>;
  return <video ref={videoRef} controls playsInline className={className ?? "w-full rounded-lg bg-black"} />;
}
