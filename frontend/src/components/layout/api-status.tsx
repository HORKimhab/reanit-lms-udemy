"use client";

import { useQuery } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
import { api } from "@/lib/api";

export function ApiStatus() {
  const { data, isError, isLoading } = useQuery({
    queryKey: ["health"],
    queryFn: () => api<{ status: string }>("/health"),
    retry: false,
  });
  if (isLoading) return <Badge variant="secondary">API: checking…</Badge>;
  if (isError || data?.status !== "ok")
    return <Badge variant="destructive">API: offline</Badge>;
  return <Badge>API: online</Badge>;
}
