import { Suspense } from "react";
import { RequireAuth } from "@/components/auth/guards";

export default function InstructorLayout({ children }: { children: React.ReactNode }) {
  return (
    <Suspense>
      <RequireAuth roles={["instructor"]}>{children}</RequireAuth>
    </Suspense>
  );
}
