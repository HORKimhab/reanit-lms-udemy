"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, del, post, type CourseCard, type Currency } from "@/lib/api";
import { useAuth } from "@/lib/auth";

// ---------- types (mirror backend/src/modules/{cart,payments}) ----------

export type Total = { currency: Currency; amount_minor: number };

export type PricedLine = {
  course_id: string;
  list_price_minor: number;
  price_minor: number;
  coupon_code: string | null;
};

export type Cart = {
  data: CourseCard[];
  pricing: PricedLine[];
  totals: Total[];
  invalid_coupons: string[];
};

export type MyCourse = { course: CourseCard; enrolled_at: string };

export type OrderStatus = "pending" | "paid" | "expired";
export type PaymentStatus = "pending" | "paid" | "failed" | "expired";

export type OrderItem = {
  course_id: string;
  title: string;
  list_price_minor: number;
  price_minor: number;
  coupon_code: string | null;
};

export type Order = {
  id: string;
  user_id: string;
  status: OrderStatus;
  currency: Currency;
  subtotal_minor: number;
  total_minor: number;
  items: OrderItem[];
  created_at: string;
  paid_at: string | null;
};

export type Payment = {
  id: string;
  method: string;
  status: PaymentStatus;
  amount_minor: number;
  currency: Currency;
  reference: string;
  failure_reason: string | null;
  qr: { string: string; svg: string } | null;
  deeplink: string | null;
  redirect: { url: string; fields: Record<string, string> } | null;
  expires_at: string;
  created_at: string;
  paid_at: string | null;
};

export type OrderView = Order & { payment: Payment | null };

export type ProviderKind = "bakong" | "aba_payway";

export type PaymentMethod = {
  code: string;
  provider: ProviderKind;
  label: string;
  description: string;
  enabled: boolean;
  currencies: Currency[];
  sort_order: number;
};

// ---------- queries ----------

/** Coupon codes as the API expects them: trimmed, upper-case, unique. */
export function normalizeCoupons(codes: string[]): string[] {
  return [...new Set(codes.map((c) => c.trim().toUpperCase()).filter(Boolean))];
}

export function useCart(coupons: string[] = []) {
  const { user } = useAuth();
  const codes = normalizeCoupons(coupons);
  const qs = codes.length ? `?coupons=${encodeURIComponent(codes.join(","))}` : "";
  return useQuery({
    queryKey: ["cart", codes],
    queryFn: () => api<Cart>(`/me/cart${qs}`),
    enabled: !!user,
  });
}

export function useWishlist() {
  const { user } = useAuth();
  return useQuery({
    queryKey: ["wishlist"],
    queryFn: () => api<{ data: CourseCard[] }>("/me/wishlist"),
    enabled: !!user,
  });
}

export function useMyCourses() {
  const { user } = useAuth();
  return useQuery({
    queryKey: ["my-courses"],
    queryFn: () => api<{ data: MyCourse[] }>("/me/courses"),
    enabled: !!user,
  });
}

export type SavedList = "cart" | "wishlist";

/** Add/remove a course on the cart or wishlist, refreshing both lists. */
export function useSavedList(list: SavedList) {
  const queryClient = useQueryClient();
  const onSuccess = () => queryClient.invalidateQueries({ queryKey: [list] });
  const add = useMutation({
    mutationFn: (courseId: string) => post(`/me/${list}`, { course_id: courseId }),
    onSuccess,
  });
  const remove = useMutation({
    mutationFn: (courseId: string) => del(`/me/${list}/${courseId}`),
    onSuccess,
  });
  return { add, remove };
}
