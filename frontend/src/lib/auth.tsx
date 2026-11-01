"use client";

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import { post, refreshSession, setAccessToken, type Session, type User } from "@/lib/api";

type AuthState = {
  user: User | null;
  /** True until the initial session restore has finished. */
  loading: boolean;
  login: (email: string, password: string) => Promise<User>;
  register: (name: string, email: string, password: string) => Promise<User>;
  logout: () => Promise<void>;
  setUser: (user: User) => void;
};

const AuthContext = createContext<AuthState | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    refreshSession()
      .then((session) => setUser(session?.user ?? null))
      .finally(() => setLoading(false));
  }, []);

  const applySession = useCallback((session: Session) => {
    setAccessToken(session.access_token);
    setUser(session.user);
    return session.user;
  }, []);

  const value = useMemo<AuthState>(
    () => ({
      user,
      loading,
      setUser,
      login: async (email, password) => applySession(await post<Session>("/auth/login", { email, password })),
      register: async (name, email, password) =>
        applySession(await post<Session>("/auth/register", { name, email, password })),
      logout: async () => {
        await post("/auth/logout").catch(() => undefined);
        setAccessToken(null);
        setUser(null);
      },
    }),
    [user, loading, applySession],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}
