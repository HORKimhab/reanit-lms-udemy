import { RedirectIfAuthenticated } from "@/components/auth/guards";

export default function AuthLayout({ children }: { children: React.ReactNode }) {
  return <RedirectIfAuthenticated>{children}</RedirectIfAuthenticated>;
}
