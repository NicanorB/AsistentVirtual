import type { ApiErrorResponse, TokenPair } from "../types/auth";

const ACCESS_TOKEN_KEY = "auth.access_token";
const REFRESH_TOKEN_KEY = "auth.refresh_token";
const TOKEN_TYPE_KEY = "auth.token_type";
const EXPIRES_IN_KEY = "auth.expires_in_seconds";
const USERNAME_KEY = "auth.username";

export function getStoredAuth() {
  return {
    accessToken: localStorage.getItem(ACCESS_TOKEN_KEY) ?? "",
    refreshToken: localStorage.getItem(REFRESH_TOKEN_KEY) ?? "",
    tokenType: localStorage.getItem(TOKEN_TYPE_KEY) ?? "Bearer",
    expiresInSeconds: Number(localStorage.getItem(EXPIRES_IN_KEY) ?? "0"),
    username: localStorage.getItem(USERNAME_KEY) ?? "",
  };
}

export function storeAuth(tokens: TokenPair, username?: string) {
  localStorage.setItem(ACCESS_TOKEN_KEY, tokens.access_token);
  localStorage.setItem(REFRESH_TOKEN_KEY, tokens.refresh_token);
  localStorage.setItem(TOKEN_TYPE_KEY, tokens.token_type);
  localStorage.setItem(EXPIRES_IN_KEY, String(tokens.expires_in_seconds ?? 0));

  if (typeof username === "string") {
    localStorage.setItem(USERNAME_KEY, username);
  }
}

export function clearStoredAuth() {
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  localStorage.removeItem(TOKEN_TYPE_KEY);
  localStorage.removeItem(EXPIRES_IN_KEY);
  localStorage.removeItem(USERNAME_KEY);
}

export async function parseApiResponse<T>(response: Response): Promise<T> {
  if (response.ok) {
    return (await response.json()) as T;
  }

  let message = `Request failed with status ${response.status}`;
  try {
    const body = (await response.json()) as ApiErrorResponse;
    message = body.error?.message ?? message;
  } catch {
    message = response.statusText || message;
  }

  throw new Error(message);
}

export function decodeJwtSub(token: string): string | null {
  try {
    const parts = token.split(".");
    if (parts.length < 2) return null;

    const payload = parts[1]!;
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    const decoded = atob(normalized);
    const json = JSON.parse(decoded) as { sub?: string };

    return typeof json.sub === "string" ? json.sub : null;
  } catch {
    return null;
  }
}
