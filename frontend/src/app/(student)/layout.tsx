import { RequireAuth } from "@/components/auth/guards";

export default function StudentLayout({ children }: { children: React.ReactNode }) {
  return <RequireAuth>{children}</RequireAuth>;
}
