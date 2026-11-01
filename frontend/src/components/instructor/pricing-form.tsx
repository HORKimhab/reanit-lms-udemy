"use client";

import { useState } from "react";
import { FormError } from "@/components/auth/auth-card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { NativeSelect } from "@/components/ui/native-select";
import { ApiError, formatPrice, type Course, type Currency } from "@/lib/api";
import { useUpdateCourse } from "./use-course";

/** USD is entered in dollars (2 decimals), KHR in whole riel. */
const toMinor = (major: string, currency: Currency) =>
  currency === "USD" ? Math.round(parseFloat(major || "0") * 100) : Math.round(parseFloat(major || "0"));
const toMajor = (minor: number, currency: Currency) =>
  currency === "USD" ? (minor / 100).toFixed(2) : String(minor);

export function PricingForm({ course, locked }: { course: Course; locked: boolean }) {
  const update = useUpdateCourse(course.id);
  const [currency, setCurrency] = useState<Currency>(course.currency);
  const [amount, setAmount] = useState(toMajor(course.price_minor, course.currency));
  const minor = toMinor(amount, currency);

  return (
    <form
      className="max-w-md space-y-4"
      onSubmit={(e) => {
        e.preventDefault();
        update.mutate({ currency, price_minor: minor });
      }}
    >
      <fieldset disabled={locked} className="space-y-4">
        <div className="grid grid-cols-3 gap-3">
          <div className="space-y-2">
            <Label htmlFor="currency">Currency</Label>
            <NativeSelect
              id="currency"
              value={currency}
              onChange={(e) => {
                const next = e.target.value as Currency;
                setAmount(toMajor(toMinor(amount, currency), next));
                setCurrency(next);
              }}
            >
              <option value="USD">USD ($)</option>
              <option value="KHR">KHR (៛)</option>
            </NativeSelect>
          </div>
          <div className="col-span-2 space-y-2">
            <Label htmlFor="price">Price</Label>
            <Input
              id="price"
              type="number"
              min={0}
              step={currency === "USD" ? "0.01" : "100"}
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
          </div>
        </div>
        <p className="text-sm text-muted-foreground">
          Students will see: <strong>{formatPrice(minor, currency)}</strong>. Set 0 for a free course.
        </p>
        <Button type="submit" disabled={update.isPending}>
          {update.isPending ? "Saving…" : "Save price"}
        </Button>
      </fieldset>
      <FormError message={update.error instanceof ApiError ? update.error.message : update.error ? "Could not save." : null} />
    </form>
  );
}
