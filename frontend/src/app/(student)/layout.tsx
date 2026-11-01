import { Suspense } from "react";
import { RequireAuth } from "@/components/auth/guards";

export default function StudentLayout({ children }: { children: React.ReactNode }) {
  return (
    <Suspense>
      <RequireAuth>{children}</RequireAuth>
    </Suspense>
  );
}
