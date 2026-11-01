import { Badge } from "@/components/ui/badge";
import type { CourseStatus } from "@/lib/api";

const LABELS: Record<CourseStatus, { label: string; variant: "default" | "secondary" | "destructive" | "outline" }> = {
  draft: { label: "Draft", variant: "secondary" },
  pending: { label: "In review", variant: "outline" },
  published: { label: "Live", variant: "default" },
  rejected: { label: "Changes requested", variant: "destructive" },
};

export function StatusBadge({ status }: { status: CourseStatus }) {
  const { label, variant } = LABELS[status];
  return <Badge variant={variant}>{label}</Badge>;
}
