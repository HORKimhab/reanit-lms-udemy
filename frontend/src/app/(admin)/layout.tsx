import { Suspense } from "react";
import { RequireAuth } from "@/components/auth/guards";

export default function AdminLayout({ children }: { children: React.ReactNode }) {
  return (
    <Suspense>
      <RequireAuth roles={["admin"]}>{children}</RequireAuth>
    </Suspense>
  );
}
