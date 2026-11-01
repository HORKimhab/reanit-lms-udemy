import Link from "next/link";
import { Input } from "@/components/ui/input";
import { VerifyEmailBanner } from "@/components/auth/verify-email-banner";
import { UserMenu } from "./user-menu";

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur">
      <div className="mx-auto flex h-16 max-w-7xl items-center gap-4 px-4">
        <Link href="/" className="text-xl font-bold tracking-tight">
          Rean<span className="text-primary">IT</span>
        </Link>
        <nav className="hidden md:block">
          <Link href="/courses" className="text-sm text-muted-foreground hover:text-foreground">
            Categories
          </Link>
        </nav>
        <form action="/courses" className="flex-1">
          <Input name="q" placeholder="Search for anything" className="rounded-full" />
        </form>
        <Link href="/teach" className="hidden text-sm text-muted-foreground hover:text-foreground md:block">
          Teach on ReanIT
        </Link>
        <UserMenu />
      </div>
      <VerifyEmailBanner />
    </header>
  );
}
