import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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

type StrengthLevel = {
  width: string;
  color: string;
  text: string;
};

type DocumentRow = {
  id: string;
  title: string;
  file: string;
};

type SuccessOverlayState = {
  show: boolean;
  title: string;
  sub: string;
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

function computePasswordStrength(val: string): StrengthLevel {
  if (!val) {
    return { width: "0%", color: "rgba(0, 212, 255, 1)", text: "" };
  }

  let score = 0;
  if (val.length >= 8) score++;
  if (/[A-Z]/.test(val)) score++;
  if (/[0-9]/.test(val)) score++;
  if (/[^A-Za-z0-9]/.test(val)) score++;

  const levels = [
    { width: "20%", color: "#ff4a6e", text: "WEAK" },
    { width: "45%", color: "#ff8c42", text: "FAIR" },
    { width: "70%", color: "#f9c74f", text: "GOOD" },
    { width: "100%", color: "#00d4ff", text: "STRONG" },
  ];

  const idx = Math.max(0, score - 1);
  return levels[idx] ?? levels[0]!;
}

function decodeJwtSub(token: string): string | null {
  try {
    const parts = token.split(".");
    if (parts.length < 2) return null;

    // base64url -> base64
    const payload = parts[1]!;
    const normalized = payload.replace(/-/g, "+").replace(/_/g, "/");
    const decoded = atob(normalized);
    const json = JSON.parse(decoded) as { sub?: string };
    return typeof json.sub === "string" ? json.sub : null;
  } catch {
    return null;
  }
}

function EyeIcon({ off }: { off: boolean }) {
  if (off) {
    return (
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" />
        <line x1="1" y1="1" x2="23" y2="23" />
      </svg>
    );
  }

  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path>
      <circle cx="12" cy="12" r="3"></circle>
    </svg>
  );
}

function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
      <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
      <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" />
      <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
    </svg>
  );
}

function GitHubIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

export default function AppAuth() {
  const storedAuth = useMemo(() => getStoredAuth(), []);
  const [view, setView] = useState<"auth" | "dashboard">(
    storedAuth.accessToken ? "dashboard" : "auth",
  );

  const [mode, setMode] = useState<AuthMode>("login");

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [email, setEmail] = useState("");
  const [password2, setPassword2] = useState("");

  const [loginPassVisible, setLoginPassVisible] = useState(false);
  const [regPassVisible, setRegPassVisible] = useState(false);
  const [regPass2Visible, setRegPass2Visible] = useState(false);

  const [rememberMe, setRememberMe] = useState(false);
  const [isSubmittingAuth, setIsSubmittingAuth] = useState(false);
  const [isRefreshing, setIsRefreshing] = useState(false);

  const [accessToken, setAccessToken] = useState(storedAuth.accessToken);
  const [refreshToken, setRefreshToken] = useState(storedAuth.refreshToken);
  const [tokenType, setTokenType] = useState(storedAuth.tokenType);
  const [expiresInSeconds, setExpiresInSeconds] = useState(
    storedAuth.expiresInSeconds,
  );

  const [loginUserErr, setLoginUserErr] = useState("");
  const [loginPassErr, setLoginPassErr] = useState("");
  const [regNameErr, setRegNameErr] = useState("");
  const [regEmailErr, setRegEmailErr] = useState("");
  const [regPassErr, setRegPassErr] = useState("");
  const [regPass2Err, setRegPass2Err] = useState("");
  const [formErr, setFormErr] = useState("");

  const [strength, setStrength] = useState<StrengthLevel>({
    width: "0%",
    color: "#00d4ff",
    text: "",
  });

  const [successOverlay, setSuccessOverlay] = useState<SuccessOverlayState>({
    show: false,
    title: "",
    sub: "",
  });
  const successTimeoutRef = useRef<number | null>(null);

  const [docsLoading, setDocsLoading] = useState(false);
  const [docsError, setDocsError] = useState("");
  const [documents, setDocuments] = useState<DocumentRow[]>([]);

  const clearAuthErrors = useCallback(() => {
    setLoginUserErr("");
    setLoginPassErr("");
    setRegNameErr("");
    setRegEmailErr("");
    setRegPassErr("");
    setRegPass2Err("");
    setFormErr("");
  }, []);

  const updateSessionState = useCallback((tokens: TokenPair) => {
    storeAuth(tokens);
    setAccessToken(tokens.access_token);
    setRefreshToken(tokens.refresh_token);
    setTokenType(tokens.token_type);
    setExpiresInSeconds(tokens.expires_in_seconds);
  }, []);

  const resetSession = useCallback(() => {
    clearStoredAuth();
    setAccessToken("");
    setRefreshToken("");
    setTokenType("Bearer");
    setExpiresInSeconds(0);
    setDocuments([]);
    setDocsError("");
  }, []);

  const refreshSession = useCallback(
    async (options?: { silent?: boolean }) => {
      if (!refreshToken) {
        throw new Error("No refresh token available.");
      }

      const shouldUpdateUi = !options?.silent;
      if (shouldUpdateUi) {
        setFormErr("");
        setDocsError("");
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

        updateSessionState(tokens);
        return tokens;
      } catch (error) {
        resetSession();
        const message =
          error instanceof Error
            ? error.message
            : "Refresh token request failed.";
        setDocsError(message);
        throw new Error(message);
      } finally {
        setIsRefreshing(false);
      }
    },
    [refreshToken, resetSession, updateSessionState],
  );

  const fetchDocuments = useCallback(async () => {
    setDocsLoading(true);
    setDocsError("");
    try {
      const requestOnce = async () => {
        const response = await fetch("/api/documents", {
          method: "GET",
          headers: {
            Authorization: `${tokenType} ${accessToken}`,
          },
        });
        if (response.status === 401) return null;
        return await parseApiResponse<DocumentRow[]>(response);
      };

      let data = await requestOnce();
      if (!data) {
        const tokens = await refreshSession({ silent: true });
        data = await parseApiResponse<DocumentRow[]>(
          await fetch("/api/documents", {
            method: "GET",
            headers: {
              Authorization: `${tokens.token_type} ${tokens.access_token}`,
            },
          }),
        );
      }

      setDocuments(data);
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Failed to load documents.";
      setDocsError(message);
      // If refresh cleared the session, return to auth.
      const storedAccess = localStorage.getItem(ACCESS_TOKEN_KEY);
      if (!storedAccess) setView("auth");
    } finally {
      setDocsLoading(false);
    }
  }, [accessToken, refreshSession, tokenType]);

  useEffect(() => {
    // Keep password strength bar in sync with the signup password input.
    setStrength(computePasswordStrength(password));
  }, [password]);

  useEffect(() => {
    // Restore session via refresh token if possible.
    if (!storedAuth.accessToken && storedAuth.refreshToken) {
      (async () => {
        try {
          await refreshSession({ silent: true });
          setView("dashboard");
        } catch {
          setView("auth");
        }
      })();
    } else if (storedAuth.accessToken) {
      setView("dashboard");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (view === "dashboard" && accessToken) {
      fetchDocuments();
    }
  }, [accessToken, fetchDocuments, view]);

  useEffect(() => {
    return () => {
      if (successTimeoutRef.current) {
        window.clearTimeout(successTimeoutRef.current);
      }
    };
  }, []);

  const handleSocialClick = (provider: string) => {
    setFormErr(`${provider} sign-in is not available in this demo.`);
  };

  const switchMode = (next: AuthMode) => {
    clearAuthErrors();
    setMode(next);
    // Reset password visibility for a cleaner tab switch.
    setLoginPassVisible(false);
    setRegPassVisible(false);
    setRegPass2Visible(false);
    setFormErr("");
  };

  const handleLoginOrSignup = async (event: FormEvent) => {
    event.preventDefault();
    clearAuthErrors();

    setIsSubmittingAuth(true);
    try {
      if (!username.trim() || !password.trim()) {
        if (mode === "login") {
          setLoginUserErr(!username.trim() ? "User ID required" : "");
          setLoginPassErr(!password.trim() ? "Access code required" : "");
        } else {
          setRegNameErr(!username.trim() ? "Display name required" : "");
          setRegPassErr(!password.trim() ? "Minimum 8 characters" : "");
        }
        return;
      }

      if (mode === "signup") {
        const trimmedName = username.trim();
        const pass = password;
        const pass2 = password2;

        let valid = true;
        if (!trimmedName) {
          setRegNameErr("Display name required");
          valid = false;
        }
        if (pass.length < 8) {
          setRegPassErr("Minimum 8 characters");
          valid = false;
        }
        if (pass !== pass2) {
          setRegPass2Err("Codes do not match");
          valid = false;
        }

        if (!valid) return;

        // Backend signup only needs `username` and `password` (email is UI-only).
        const tokens = await parseApiResponse<TokenPair>(
          await fetch("/api/auth/signup", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              username: trimmedName,
              password: pass,
            }),
          }),
        );

        updateSessionState(tokens);
        setSuccessOverlay({
          show: true,
          title: "IDENTITY REGISTERED",
          sub: "Redirecting to interface…",
        });
      } else {
        const trimmedUser = username.trim();
        const pass = password;

        if (!trimmedUser) {
          setLoginUserErr("User ID required");
          return;
        }
        if (!pass) {
          setLoginPassErr("Access code required");
          return;
        }

        const tokens = await parseApiResponse<TokenPair>(
          await fetch("/api/auth/login", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              username: trimmedUser,
              password: pass,
            }),
          }),
        );

        updateSessionState(tokens);
        setSuccessOverlay({
          show: true,
          title: "ACCESS GRANTED",
          sub: "Redirecting to interface…",
        });
      }

      // Small delay to let the overlay be seen.
      successTimeoutRef.current = window.setTimeout(() => {
        setSuccessOverlay((s) => ({ ...s, show: false }));
        setView("dashboard");
      }, 1200);
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Authentication failed.";

      if (mode === "login") {
        setLoginPassErr(message);
      } else {
        const lowered = message.toLowerCase();
        if (
          lowered.includes("username") ||
          lowered.includes("exists") ||
          lowered.includes("conflict")
        ) {
          setRegNameErr(message);
        } else {
          setRegPassErr(message);
        }
      }
      setFormErr(message);
    } finally {
      setIsSubmittingAuth(false);
    }
  };

  const handleLogout = () => {
    resetSession();
    clearAuthErrors();
    setSuccessOverlay({ show: false, title: "", sub: "" });
    setView("auth");
    setMode("login");
    setUsername("");
    setPassword("");
    //setEmail("");
    setPassword2("");
    setDocuments([]);
  };

  const loginActive = mode === "login";
  const registerActive = mode === "signup";

  const passwordStrengthStyle = {
    width: strength.width,
    background: strength.color,
  } as const;

  const panelSuccessSvg = (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <polyline points="20 6 9 17 4 12"></polyline>
    </svg>
  );

  if (view === "dashboard") {
    const sub = accessToken ? decodeJwtSub(accessToken) : null;

    return (
      <div className="dashboard-page">
        <div className="bg">
          <div className="ring ring-outer-l"></div>
          <div className="ring ring-inner-l"></div>
          <div className="ring ring-outer-r"></div>
          <div className="ring ring-inner-r"></div>
          <div className="beam beam-top"></div>
          <div className="beam beam-bottom"></div>
          <div className="orb orb-left"></div>
          <div className="orb orb-right"></div>
        </div>
        <div className="scan-line"></div>

        <div className="dashboard-shell">
          <div className="dashboard-header">
            <div>
              <div className="dashboard-title">MOCK DASHBOARD</div>
              <div className="dashboard-badge">
                {sub ? `user: ${sub}` : "authenticated"} · expires:{" "}
                {expiresInSeconds ? `${expiresInSeconds}s` : "-"}
              </div>
            </div>

            <div className="dashboard-actions">
              <button
                className="dash-btn danger"
                type="button"
                onClick={handleLogout}
                disabled={isRefreshing}
              >
                Logout
              </button>
            </div>
          </div>

          <div className="dashboard-card">
            <div className="dashboard-grid">
              <div>
                <div className="dashboard-section-title">Your Documents</div>
                <div className="dashboard-note">
                  {docsLoading
                    ? "Loading…"
                    : docsError
                      ? "Unable to fetch documents."
                      : "Fetched from backend."}
                </div>

                {docsError ? (
                  <div className="dashboard-error" style={{ marginTop: 10 }}>
                    {docsError}
                  </div>
                ) : null}

                {!docsLoading && !docsError ? (
                  <div className="document-list" style={{ marginTop: 12 }}>
                    {documents.length ? (
                      documents.map((doc) => (
                        <div className="document-row" key={doc.id}>
                          <div className="document-main">
                            <div className="document-title" title={doc.title}>
                              {doc.title}
                            </div>
                            <div className="document-meta">
                              file: {doc.file}
                            </div>
                          </div>
                          <div
                            className="document-meta"
                            style={{ whiteSpace: "nowrap" }}
                          >
                            {doc.id}
                          </div>
                        </div>
                      ))
                    ) : (
                      <div className="dashboard-note">No documents found.</div>
                    )}
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-page">
      <div className="bg">
        <div className="ring ring-outer-l"></div>
        <div className="ring ring-inner-l"></div>
        <div className="ring ring-outer-r"></div>
        <div className="ring ring-inner-r"></div>
        <div className="beam beam-top"></div>
        <div className="beam beam-bottom"></div>
        <div className="orb orb-left"></div>
        <div className="orb orb-right"></div>
      </div>
      <div className="scan-line"></div>

      <div className="panel-wrap">
        <div className="welcome-title">AI &nbsp; INTERFACE</div>

        <div className="panel">
          <div
            className={`success-overlay ${successOverlay.show ? "show" : ""}`}
          >
            <div className="success-icon">{panelSuccessSvg}</div>
            <div className="success-title">{successOverlay.title}</div>
            <div className="success-sub">{successOverlay.sub}</div>
          </div>

          <div className="id-bar">
            <div className="id-dot"></div>
            <div className="id-line"></div>
            <div className="id-text">Secure Auth Module</div>
            <div className="id-line"></div>
            <div className="id-dot" style={{ animationDelay: ".8s" }}></div>
          </div>

          <div className="tabs">
            <button
              type="button"
              className={`tab-btn ${loginActive ? "active" : ""}`}
              onClick={() => switchMode("login")}
            >
              Sign In
            </button>
            <button
              type="button"
              className={`tab-btn ${registerActive ? "active" : ""}`}
              onClick={() => switchMode("signup")}
            >
              Register
            </button>
          </div>

          <div className="form-wrap">
            {/* ── LOGIN FORM ── */}
            <div
              className={`form-panel ${loginActive ? "active" : ""}`}
              id="form-login"
            >
              <form onSubmit={handleLoginOrSignup}>
                <div className="field">
                  <label>User ID</label>
                  <div className="input-wrap">
                    <svg
                      className="input-icon"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2.0"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
                      <circle cx="12" cy="7" r="4"></circle>
                    </svg>
                    <input
                      type="text"
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                      placeholder="Enter your username"
                      autoComplete="username"
                      disabled={isSubmittingAuth || successOverlay.show}
                      className={loginUserErr ? "error-field" : ""}
                    />
                  </div>
                  <div className="error-msg">{loginUserErr}</div>
                </div>

                <div className="field">
                  <label>Password</label>
                  <div className="input-wrap">
                    <svg
                      className="input-icon"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <rect
                        x="3"
                        y="11"
                        width="18"
                        height="11"
                        rx="2"
                        ry="2"
                      ></rect>
                      <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                    </svg>
                    <input
                      type={loginPassVisible ? "text" : "password"}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      placeholder="Enter your password"
                      autoComplete="current-password"
                      disabled={isSubmittingAuth || successOverlay.show}
                      className={loginPassErr ? "error-field" : ""}
                    />
                    <button
                      type="button"
                      className="toggle-pass"
                      onClick={() => setLoginPassVisible((v) => !v)}
                      tabIndex={-1}
                      disabled={isSubmittingAuth || successOverlay.show}
                    >
                      <EyeIcon off={!loginPassVisible} />
                    </button>
                  </div>
                  <div className="error-msg">{loginPassErr}</div>
                </div>

                <div className="row-options">
                  <label className="remember">
                    <input
                      type="checkbox"
                      checked={rememberMe}
                      onChange={(e) => setRememberMe(e.target.checked)}
                      disabled={isSubmittingAuth || successOverlay.show}
                    />
                    <div className="custom-check"></div>
                    <span>Remember me</span>
                  </label>
                  <a
                    className="forgot-link"
                    href="#"
                    onClick={(e) => {
                      e.preventDefault();
                      setFormErr(
                        "Forgot access code? Not implemented in this demo.",
                      );
                    }}
                  >
                    Forgot access code?
                  </a>
                </div>

                <button
                  type="submit"
                  className={`submit-btn ${isSubmittingAuth ? "loading" : ""}`}
                  disabled={isSubmittingAuth}
                >
                  Initiate Connection
                </button>

                {formErr ? (
                  <div className="error-msg" style={{ marginTop: 10 }}>
                    {formErr}
                  </div>
                ) : null}

                <div className="or-divider">
                  <span>or continue with</span>
                </div>

                <div className="social-row">
                  <button
                    type="button"
                    className="social-btn"
                    disabled
                    onClick={() => handleSocialClick("Google")}
                  >
                    <GoogleIcon />
                    Google
                  </button>
                  <button
                    type="button"
                    className="social-btn"
                    disabled
                    onClick={() => handleSocialClick("GitHub")}
                  >
                    <GitHubIcon />
                    GitHub
                  </button>
                </div>
              </form>
            </div>

            {/* ── REGISTER FORM ── */}
            <div
              className={`form-panel ${registerActive ? "active" : ""}`}
              id="form-register"
            >
              <form onSubmit={handleLoginOrSignup}>
                <div className="field">
                  <label>Name</label>
                  <div className="input-wrap">
                    <svg
                      className="input-icon"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path>
                      <circle cx="12" cy="7" r="4"></circle>
                    </svg>
                    <input
                      type="text"
                      value={username}
                      onChange={(e) => setUsername(e.target.value)}
                      placeholder="Choose a username"
                      autoComplete="username"
                      disabled={isSubmittingAuth || successOverlay.show}
                      className={regNameErr ? "error-field" : ""}
                    />
                  </div>
                  <div className="error-msg">{regNameErr}</div>
                </div>

                <div className="field">
                  <label>Access Code</label>
                  <div className="input-wrap">
                    <svg
                      className="input-icon"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <rect
                        x="3"
                        y="11"
                        width="18"
                        height="11"
                        rx="2"
                        ry="2"
                      ></rect>
                      <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                    </svg>
                    <input
                      type={regPassVisible ? "text" : "password"}
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      placeholder="Create a password"
                      autoComplete="new-password"
                      disabled={isSubmittingAuth || successOverlay.show}
                      className={regPassErr ? "error-field" : ""}
                    />
                    <button
                      type="button"
                      className="toggle-pass"
                      onClick={() => setRegPassVisible((v) => !v)}
                      tabIndex={-1}
                      disabled={isSubmittingAuth || successOverlay.show}
                    >
                      <EyeIcon off={!regPassVisible} />
                    </button>
                  </div>

                  <div className="strength-bar-wrap">
                    <div
                      className="strength-bar"
                      style={passwordStrengthStyle}
                    ></div>
                  </div>
                  <div
                    className="strength-label"
                    style={{ color: strength.color }}
                  >
                    {strength.text}
                  </div>
                  <div className="error-msg">{regPassErr}</div>
                </div>

                <div className="field" style={{ marginBottom: 20 }}>
                  <label>Confirm Password</label>
                  <div className="input-wrap">
                    <svg
                      className="input-icon"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <rect
                        x="3"
                        y="11"
                        width="18"
                        height="11"
                        rx="2"
                        ry="2"
                      ></rect>
                      <path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                    </svg>
                    <input
                      type={regPass2Visible ? "text" : "password"}
                      value={password2}
                      onChange={(e) => setPassword2(e.target.value)}
                      placeholder="Repeat your password"
                      autoComplete="new-password"
                      disabled={isSubmittingAuth || successOverlay.show}
                      className={regPass2Err ? "error-field" : ""}
                    />
                    <button
                      type="button"
                      className="toggle-pass"
                      onClick={() => setRegPass2Visible((v) => !v)}
                      tabIndex={-1}
                      disabled={isSubmittingAuth || successOverlay.show}
                    >
                      <EyeIcon off={!regPass2Visible} />
                    </button>
                  </div>
                  <div className="error-msg">{regPass2Err}</div>
                </div>

                <button
                  type="submit"
                  className={`submit-btn ${isSubmittingAuth ? "loading" : ""}`}
                  disabled={isSubmittingAuth}
                >
                  Create Account
                </button>

                {formErr ? (
                  <div className="error-msg" style={{ marginTop: 10 }}>
                    {formErr}
                  </div>
                ) : null}

                <div className="or-divider">
                  <span>or continue with</span>
                </div>
              </form>
            </div>
          </div>

          <div className="bottom-note" id="bottom-note">
            {mode === "login" ? (
              <>
                Nu ai cont?{" "}
                <span
                  onClick={() => switchMode("signup")}
                  role="button"
                  tabIndex={0}
                >
                  Înregistrează-te
                </span>
              </>
            ) : (
              <>
                Ai deja cont?{" "}
                <span
                  onClick={() => switchMode("login")}
                  role="button"
                  tabIndex={0}
                >
                  Conectează-te
                </span>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
