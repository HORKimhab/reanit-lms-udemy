import { Suspense } from "react";
import { RedirectIfAuthenticated } from "@/components/auth/guards";

export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return (
    <Suspense>
      <RedirectIfAuthenticated>{children}</RedirectIfAuthenticated>
    </Suspense>
  );
}
