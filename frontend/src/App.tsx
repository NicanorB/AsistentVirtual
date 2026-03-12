import { useMemo, useState } from "react";
import type { FormEvent } from "react";
import "./App.css";

type TokenPair = {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in_seconds: number;
};

type ApiErrorResponse = {
  error?: {
    code?: string;
    message?: string;
  };
};

type AuthMode = "login" | "signup";

type ProtectedResponse = {
  ok: boolean;
  user_id: string;
};

const ACCESS_TOKEN_KEY = "auth.access_token";
const REFRESH_TOKEN_KEY = "auth.refresh_token";
const TOKEN_TYPE_KEY = "auth.token_type";
const EXPIRES_IN_KEY = "auth.expires_in_seconds";

function getStoredAuth() {
  return {
    accessToken: localStorage.getItem(ACCESS_TOKEN_KEY) ?? "",
    refreshToken: localStorage.getItem(REFRESH_TOKEN_KEY) ?? "",
    tokenType: localStorage.getItem(TOKEN_TYPE_KEY) ?? "Bearer",
    expiresInSeconds: Number(localStorage.getItem(EXPIRES_IN_KEY) ?? "0"),
  };
}

function storeAuth(tokens: TokenPair) {
  localStorage.setItem(ACCESS_TOKEN_KEY, tokens.access_token);
  localStorage.setItem(REFRESH_TOKEN_KEY, tokens.refresh_token);
  localStorage.setItem(TOKEN_TYPE_KEY, tokens.token_type);
  localStorage.setItem(EXPIRES_IN_KEY, String(tokens.expires_in_seconds ?? 0));
}

function clearStoredAuth() {
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
  localStorage.removeItem(TOKEN_TYPE_KEY);
  localStorage.removeItem(EXPIRES_IN_KEY);
}

async function parseApiResponse<T>(response: Response): Promise<T> {
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

function App() {
  const storedAuth = useMemo(() => getStoredAuth(), []);
  const [mode, setMode] = useState<AuthMode>("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [accessToken, setAccessToken] = useState(storedAuth.accessToken);
  const [refreshToken, setRefreshToken] = useState(storedAuth.refreshToken);
  const [tokenType, setTokenType] = useState(storedAuth.tokenType);
  const [expiresInSeconds, setExpiresInSeconds] = useState(
    storedAuth.expiresInSeconds,
  );
  const [statusMessage, setStatusMessage] = useState(
    storedAuth.accessToken
      ? "Loaded existing session from local storage."
      : "Use signup or login to authenticate.",
  );
  const [errorMessage, setErrorMessage] = useState("");
  const [protectedResponse, setProtectedResponse] =
    useState<ProtectedResponse | null>(null);
  const [isSubmittingAuth, setIsSubmittingAuth] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isCallingProtected, setIsCallingProtected] = useState(false);

  const isAuthenticated = Boolean(accessToken);

  const updateSession = (tokens: TokenPair, successMessage: string) => {
    storeAuth(tokens);
    setAccessToken(tokens.access_token);
    setRefreshToken(tokens.refresh_token);
    setTokenType(tokens.token_type);
    setExpiresInSeconds(tokens.expires_in_seconds);
    setProtectedResponse(null);
    setErrorMessage("");
    setStatusMessage(successMessage);
  };

  const resetSession = (message: string) => {
    clearStoredAuth();
    setAccessToken("");
    setRefreshToken("");
    setTokenType("Bearer");
    setExpiresInSeconds(0);
    setProtectedResponse(null);
    setErrorMessage("");
    setStatusMessage(message);
  };

  const submitAuth = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setErrorMessage("");
    setProtectedResponse(null);

    if (!username.trim() || !password.trim()) {
      setErrorMessage("Username and password are required.");
      return;
    }

    setIsSubmittingAuth(true);

    try {
      const endpoint =
        mode === "signup" ? "/api/auth/signup" : "/api/auth/login";

      const tokens = await parseApiResponse<TokenPair>(
        await fetch(endpoint, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            username: username.trim(),
            password,
          }),
        }),
      );

      updateSession(
        tokens,
        mode === "signup"
          ? "Signup successful. You are now authenticated."
          : "Login successful.",
      );
    } catch (error) {
      setErrorMessage(
        error instanceof Error ? error.message : "Authentication failed.",
      );
    } finally {
      setIsSubmittingAuth(false);
    }
  };

  const handleRefreshToken = async () => {
    setErrorMessage("");
    setProtectedResponse(null);

    if (!refreshToken) {
      setErrorMessage("No refresh token available.");
      return;
    }

    setIsRefreshing(true);

    try {
      const tokens = await parseApiResponse<TokenPair>(
        await fetch("/api/auth/refresh_token", {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            refresh_token: refreshToken,
          }),
        }),
      );

      updateSession(tokens, "Access token refreshed successfully.");
    } catch (error) {
      resetSession("Session cleared after refresh failure.");
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Refresh token request failed.",
      );
    } finally {
      setIsRefreshing(false);
    }
  };

  const callProtectedEndpoint = async () => {
    setErrorMessage("");
    setProtectedResponse(null);

    if (!accessToken) {
      setErrorMessage("You must be logged in to call the protected endpoint.");
      return;
    }

    setIsCallingProtected(true);

    try {
      const data = await parseApiResponse<ProtectedResponse>(
        await fetch("/api/mock", {
          method: "GET",
          headers: {
            Authorization: `${tokenType} ${accessToken}`,
          },
        }),
      );

      setProtectedResponse(data);
      setStatusMessage("Protected endpoint call succeeded.");
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : "Protected endpoint request failed.",
      );
    } finally {
      setIsCallingProtected(false);
    }
  };

  const handleLogout = () => {
    resetSession("Logged out locally.");
  };

  return (
    <div className="app-shell">
      <div className="card auth-card">
        <h1>Simple Auth Demo</h1>
        <p className="read-the-docs">
          Test the backend authentication flow: signup, login, refresh token,
          protected endpoint, and logout.
        </p>

        <div className="auth-mode-switch">
          <button
            type="button"
            className={mode === "login" ? "active" : ""}
            onClick={() => setMode("login")}
          >
            Login
          </button>
          <button
            type="button"
            className={mode === "signup" ? "active" : ""}
            onClick={() => setMode("signup")}
          >
            Signup
          </button>
        </div>

        <form className="auth-form" onSubmit={submitAuth}>
          <label>
            Username
            <input
              type="text"
              value={username}
              autoComplete="username"
              onChange={(event) => setUsername(event.target.value)}
              placeholder="Enter username"
            />
          </label>

          <label>
            Password
            <input
              type="password"
              value={password}
              autoComplete={
                mode === "signup" ? "new-password" : "current-password"
              }
              onChange={(event) => setPassword(event.target.value)}
              placeholder="Enter password"
            />
          </label>

          <button type="submit" disabled={isSubmittingAuth}>
            {isSubmittingAuth
              ? mode === "signup"
                ? "Signing up..."
                : "Logging in..."
              : mode === "signup"
                ? "Signup"
                : "Login"}
          </button>
        </form>

        <div className="action-grid">
          <button
            type="button"
            onClick={handleRefreshToken}
            disabled={!refreshToken || isRefreshing}
          >
            {isRefreshing ? "Refreshing..." : "Refresh Token"}
          </button>

          <button
            type="button"
            onClick={callProtectedEndpoint}
            disabled={!isAuthenticated || isCallingProtected}
          >
            {isCallingProtected ? "Calling..." : "Call Protected Endpoint"}
          </button>

          <button
            type="button"
            onClick={handleLogout}
            disabled={!isAuthenticated && !refreshToken}
          >
            Logout
          </button>
        </div>

        <div className="status-panel">
          <h2>Session Status</h2>
          <p>
            <strong>Authenticated:</strong> {isAuthenticated ? "Yes" : "No"}
          </p>
          <p>
            <strong>Status:</strong> {statusMessage}
          </p>
          {errorMessage ? (
            <p className="error-text">
              <strong>Error:</strong> {errorMessage}
            </p>
          ) : null}
        </div>

        <div className="token-panel">
          <h2>Tokens</h2>
          <p>
            <strong>Token type:</strong> {tokenType || "-"}
          </p>
          <p>
            <strong>Access token expires in:</strong>{" "}
            {expiresInSeconds ? `${expiresInSeconds} seconds` : "-"}
          </p>

          <label>
            Access token
            <textarea
              value={accessToken}
              readOnly
              rows={6}
              placeholder="Access token will appear here after authentication."
            />
          </label>

          <label>
            Refresh token
            <textarea
              value={refreshToken}
              readOnly
              rows={6}
              placeholder="Refresh token will appear here after authentication."
            />
          </label>
        </div>

        <div className="response-panel">
          <h2>Protected Endpoint Response</h2>
          <pre>
            {protectedResponse
              ? JSON.stringify(protectedResponse, null, 2)
              : "No protected response yet."}
          </pre>
        </div>

        <div className="help-panel">
          <h2>Backend Routes Used</h2>
          <ul>
            <li>
              <code>POST /api/auth/signup</code>
            </li>
            <li>
              <code>POST /api/auth/login</code>
            </li>
            <li>
              <code>POST /api/auth/refresh_token</code>
            </li>
            <li>
              <code>GET /api/mock</code>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
}

export default App;
