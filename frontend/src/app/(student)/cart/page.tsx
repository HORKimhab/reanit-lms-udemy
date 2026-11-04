"use client";

import Link from "next/link";
import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ApiError, formatPrice, type CourseCard } from "@/lib/api";
import { normalizeCoupons, useCart, useSavedList, type PricedLine } from "@/lib/commerce";

export default function CartPage() {
  const [coupons, setCoupons] = useState<string[]>([]);
  const [code, setCode] = useState("");
  const cart = useCart(coupons);
  const saved = useSavedList("cart");
  const wishlist = useSavedList("wishlist");

  const c = cart.data;
  const lineOf = (id: string) => c?.pricing.find((p) => p.course_id === id);
  const applied = normalizeCoupons(coupons).filter((x) => !c?.invalid_coupons.includes(x));
  const error = [saved.remove.error, wishlist.add.error].find((e) => e instanceof ApiError);

  return (
    <main className="mx-auto w-full max-w-6xl px-4 py-10">
      <h1 className="mb-6 text-3xl font-bold">Shopping cart</h1>
      <FormError message={error?.message ?? null} />
      {cart.isLoading && <p className="text-muted-foreground">Loading…</p>}
      {c && c.data.length === 0 && (
        <div className="rounded-lg border p-10 text-center">
          <p className="mb-4 text-muted-foreground">Your cart is empty. Keep shopping to find a course!</p>
          <Link href="/courses" className={buttonVariants()}>
            Browse courses
          </Link>
        </div>
      )}

      {c && c.data.length > 0 && (
        <div className="grid gap-8 md:grid-cols-[1fr_320px]">
          <section>
            <p className="mb-2 text-sm font-medium">{c.data.length} course(s) in cart</p>
            <ul className="divide-y rounded-lg border">
              {c.data.map((course) => (
                <CartRow
                  key={course.id}
                  course={course}
                  line={lineOf(course.id)}
                  busy={saved.remove.isPending || wishlist.add.isPending}
                  onRemove={() => saved.remove.mutate(course.id)}
                  onMoveToWishlist={async () => {
                    await wishlist.add.mutateAsync(course.id);
                    saved.remove.mutate(course.id);
                  }}
                />
              ))}
            </ul>
          </section>

          <aside className="space-y-4 self-start">
            {c.totals.map((t) => {
              const list = c.data
                .filter((x) => x.currency === t.currency)
                .reduce((sum, x) => sum + (lineOf(x.id)?.list_price_minor ?? x.price_minor), 0);
              const qs = new URLSearchParams({ currency: t.currency });
              if (applied.length) qs.set("coupons", applied.join(","));
              return (
                <Card key={t.currency}>
                  <CardContent className="space-y-3">
                    <p className="text-sm text-muted-foreground">Total ({t.currency})</p>
                    <p className="text-3xl font-bold">{formatPrice(t.amount_minor, t.currency)}</p>
                    {list > t.amount_minor && (
                      <p className="text-sm text-muted-foreground line-through">{formatPrice(list, t.currency)}</p>
                    )}
                    <Link href={`/checkout?${qs}`} className={buttonVariants({ className: "w-full" })}>
                      Checkout {c.totals.length > 1 ? `${t.currency} courses` : ""}
                    </Link>
                  </CardContent>
                </Card>
              );
            })}
            {c.totals.length > 1 && (
              <p className="text-xs text-muted-foreground">
                Courses priced in different currencies are paid in separate orders.
              </p>
            )}

            <form
              className="space-y-2"
              onSubmit={(e) => {
                e.preventDefault();
                if (code.trim()) setCoupons((cs) => [...cs, code]);
                setCode("");
              }}
            >
              <p className="text-sm font-medium">Promotions</p>
              <div className="flex gap-2">
                <Input value={code} onChange={(e) => setCode(e.target.value)} placeholder="Enter coupon" />
                <Button type="submit" variant="outline">
                  Apply
                </Button>
              </div>
              {normalizeCoupons(coupons).map((x) => {
                const invalid = c.invalid_coupons.includes(x);
                return (
                  <p key={x} className="flex items-center justify-between text-sm">
                    <span className={invalid ? "text-destructive" : ""}>
                      <span className="font-mono">{x}</span> {invalid ? "isn't valid for these courses" : "applied"}
                    </span>
                    <button
                      type="button"
                      className="text-xs underline"
                      onClick={() => setCoupons((cs) => normalizeCoupons(cs).filter((y) => y !== x))}
                    >
                      Remove
                    </button>
                  </p>
                );
              })}
            </form>
          </aside>
        </div>
      )}
    </main>
  );
}

function CartRow({
  course,
  line,
  busy,
  onRemove,
  onMoveToWishlist,
}: {
  course: CourseCard;
  line?: PricedLine;
  busy: boolean;
  onRemove: () => void;
  onMoveToWishlist: () => void;
}) {
  const price = line?.price_minor ?? course.price_minor;
  const list = line?.list_price_minor ?? course.price_minor;
  return (
    <li className="flex gap-4 p-4">
      <div className="aspect-video w-32 shrink-0 overflow-hidden rounded border bg-muted">
        {course.thumbnail_url && (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={course.thumbnail_url} alt="" className="size-full object-cover" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <Link href={`/course/${course.slug}`} className="font-semibold hover:underline">
          {course.title}
        </Link>
        <p className="text-xs text-muted-foreground">{course.instructor_name}</p>
        <div className="mt-2 flex gap-3 text-xs">
          <button className="text-primary underline disabled:opacity-50" disabled={busy} onClick={onRemove}>
            Remove
          </button>
          <button className="text-primary underline disabled:opacity-50" disabled={busy} onClick={onMoveToWishlist}>
            Move to wishlist
          </button>
        </div>
      </div>
      <div className="text-right">
        <p className="font-bold">{formatPrice(price, course.currency)}</p>
        {list > price && (
          <p className="text-xs text-muted-foreground line-through">{formatPrice(list, course.currency)}</p>
        )}
        {line?.coupon_code && <p className="text-xs text-primary">{line.coupon_code}</p>}
      </div>
    </li>
  );
}
