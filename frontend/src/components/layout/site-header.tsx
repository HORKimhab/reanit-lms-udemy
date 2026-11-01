import Link from "next/link";
import { Input } from "@/components/ui/input";
import { VerifyEmailBanner } from "@/components/auth/verify-email-banner";
import { TeachLink, UserMenu } from "./user-menu";

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur">
      <div className="mx-auto flex h-16 max-w-7xl items-center gap-4 px-4">
        <Link href="/" className="text-xl font-bold tracking-tight">
          Rean<span className="text-primary">IT</span>
        </Link>
        <nav className="hidden md:block">
          <Link href="/courses" className="text-sm text-muted-foreground hover:text-foreground">
            Browse
          </Link>
        </nav>
        <form action="/courses" className="flex-1">
          <Input name="q" placeholder="Search for anything" className="rounded-full" />
        </form>
        <TeachLink />
        <UserMenu />
      </div>
      <VerifyEmailBanner />
    </header>
  );
}
