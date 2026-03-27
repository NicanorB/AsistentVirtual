import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import "./App.css";
import Dashboard from "./Dashboard.tsx";
import type {
  AuthMode,
  ChatMessage,
  ChatSourceItem,
  ChatStreamDone,
  ChatStreamEvent,
  DocumentRow,
  StrengthLevel,
  SuccessOverlayState,
  TokenPair,
} from "./types/auth";

const ACCEPTED_DOCUMENT_TYPES = [
  "application/pdf",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "text/plain",
] as const;
const ACCEPTED_DOCUMENT_EXTENSIONS = [".pdf", ".docx", ".txt"] as const;
import {
  clearStoredAuth,
  getStoredAuth,
  parseApiResponse,
  storeAuth,
} from "./utils/auth";
import { computePasswordStrength } from "./utils/password";

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

export default function App() {
  const storedAuth = useMemo(() => getStoredAuth(), []);
  const [dashboardUsername, setDashboardUsername] = useState(
    storedAuth.username,
  );
  const [view, setView] = useState<"auth" | "dashboard">(
    storedAuth.accessToken ? "dashboard" : "auth",
  );

  const [mode, setMode] = useState<AuthMode>("login");

  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
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

  const [loginUserErr, setLoginUserErr] = useState("");
  const [loginPassErr, setLoginPassErr] = useState("");
  const [regNameErr, setRegNameErr] = useState("");
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
  const [uploadingDocument, setUploadingDocument] = useState(false);
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([]);
  const [chatSources, setChatSources] = useState<ChatSourceItem[]>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatLoading, setChatLoading] = useState(false);
  const [chatError, setChatError] = useState("");

  const clearAuthErrors = useCallback(() => {
    setLoginUserErr("");
    setLoginPassErr("");
    setRegNameErr("");
    setRegPassErr("");
    setRegPass2Err("");
    setFormErr("");
  }, []);

  const updateSessionState = useCallback(
    (tokens: TokenPair, nextUsername?: string) => {
      storeAuth(tokens, nextUsername);
      setAccessToken(tokens.access_token);
      setRefreshToken(tokens.refresh_token);
      setTokenType(tokens.token_type);
      if (typeof nextUsername === "string") {
        setDashboardUsername(nextUsername);
      }
    },
    [],
  );

  const resetSession = useCallback(() => {
    clearStoredAuth();
    setAccessToken("");
    setRefreshToken("");
    setTokenType("Bearer");
    setDocuments([]);
    setDocsError("");
    setChatMessages([]);
    setChatSources([]);
    setChatInput("");
    setChatError("");
    setChatLoading(false);
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
      const storedAccess = getStoredAuth().accessToken;
      if (!storedAccess) setView("auth");
    } finally {
      setDocsLoading(false);
    }
  }, [accessToken, refreshSession, tokenType]);

  const handleDocumentUpload = useCallback(
    async (file: File | null) => {
      if (!file) return;

      const lowerName = file.name.toLowerCase();
      const hasAcceptedExtension = ACCEPTED_DOCUMENT_EXTENSIONS.some((ext) =>
        lowerName.endsWith(ext),
      );
      const hasAcceptedMimeType =
        !file.type ||
        ACCEPTED_DOCUMENT_TYPES.includes(
          file.type as (typeof ACCEPTED_DOCUMENT_TYPES)[number],
        );

      if (!hasAcceptedExtension || !hasAcceptedMimeType) {
        setDocsError("Only PDF, DOCX, or TXT files are allowed.");
        return;
      }

      if (!accessToken) {
        setDocsError("You must be authenticated to upload documents.");
        setView("auth");
        return;
      }

      setUploadingDocument(true);
      setDocsError("");

      const uploadOnce = async (authHeader: string) => {
        const formData = new FormData();
        formData.append("file", file);

        return await fetch("/api/documents", {
          method: "POST",
          headers: {
            Authorization: authHeader,
          },
          body: formData,
        });
      };

      try {
        let response = await uploadOnce(`${tokenType} ${accessToken}`);

        if (response.status === 401) {
          const tokens = await refreshSession({ silent: true });
          response = await uploadOnce(
            `${tokens.token_type} ${tokens.access_token}`,
          );
        }

        await parseApiResponse<DocumentRow>(response);
        await fetchDocuments();
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Failed to upload document.";
        setDocsError(message);
        const storedAccess = getStoredAuth().accessToken;
        if (!storedAccess) setView("auth");
      } finally {
        setUploadingDocument(false);
      }
    },
    [accessToken, fetchDocuments, refreshSession, tokenType],
  );

  const handleChatSubmit = useCallback(
    async (message: string) => {
      const trimmedMessage = message.trim();
      if (!trimmedMessage || !accessToken || chatLoading) return;

      const userMessage: ChatMessage = {
        id: `user-${Date.now()}`,
        role: "user",
        content: trimmedMessage,
      };
      const assistantMessageId = `assistant-${Date.now()}`;

      setChatMessages((current) => [
        ...current,
        userMessage,
        {
          id: assistantMessageId,
          role: "assistant",
          content: "",
        },
      ]);
      setChatSources([]);
      setChatError("");
      setChatInput("");
      setChatLoading(true);

      const readSseResponse = async (response: Response) => {
        if (!response.ok || !response.body) {
          throw new Error("Failed to start chat stream.");
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        const applyChunk = (payload: string) => {
          if (!payload.trim()) return;

          const event = JSON.parse(payload) as ChatStreamEvent;
          if (event.stop) {
            const doneEvent = event as ChatStreamDone;
            setChatSources(doneEvent.sources ?? []);
            return;
          }

          setChatMessages((current) =>
            current.map((entry) =>
              entry.id === assistantMessageId
                ? { ...entry, content: entry.content + event.content }
                : entry,
            ),
          );
        };

        while (true) {
          const { value, done } = await reader.read();
          buffer += decoder.decode(value ?? new Uint8Array(), {
            stream: !done,
          });

          let separatorIndex = buffer.indexOf("\n\n");
          while (separatorIndex !== -1) {
            const rawEvent = buffer.slice(0, separatorIndex);
            buffer = buffer.slice(separatorIndex + 2);

            const payload = rawEvent
              .split("\n")
              .filter((line) => line.startsWith("data: "))
              .map((line) => line.slice(6))
              .join("\n");

            if (payload) {
              applyChunk(payload);
            }

            separatorIndex = buffer.indexOf("\n\n");
          }

          if (done) {
            const remainingPayload = buffer
              .split("\n")
              .filter((line) => line.startsWith("data: "))
              .map((line) => line.slice(6))
              .join("\n");

            if (remainingPayload) {
              applyChunk(remainingPayload);
            }

            break;
          }
        }
      };

      const requestOnce = async (authHeader: string) =>
        await fetch("/api/chat/query", {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: authHeader,
          },
          body: JSON.stringify({
            query: trimmedMessage,
          }),
        });

      try {
        let response = await requestOnce(`${tokenType} ${accessToken}`);

        if (response.status === 401) {
          const tokens = await refreshSession({ silent: true });
          response = await requestOnce(
            `${tokens.token_type} ${tokens.access_token}`,
          );
        }

        await readSseResponse(response);
      } catch (error) {
        const message =
          error instanceof Error ? error.message : "Failed to send chat query.";
        setChatError(message);
        setChatMessages((current) =>
          current.map((entry) =>
            entry.id === assistantMessageId && !entry.content.trim()
              ? {
                  ...entry,
                  content:
                    "I couldn't complete that request. Please try again.",
                }
              : entry,
          ),
        );

        const storedAccess = getStoredAuth().accessToken;
        if (!storedAccess) setView("auth");
      } finally {
        setChatLoading(false);
      }
    },
    [accessToken, chatLoading, refreshSession, tokenType],
  );

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

        updateSessionState(tokens, trimmedName);
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

        updateSessionState(tokens, trimmedUser);
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
    setDashboardUsername("");
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
    return (
      <Dashboard
        username={dashboardUsername}
        accessToken={accessToken}
        isRefreshing={isRefreshing}
        docsLoading={docsLoading}
        docsError={docsError}
        documents={documents}
        onLogout={handleLogout}
        uploadingDocument={uploadingDocument}
        onUploadDocument={handleDocumentUpload}
        chatMessages={chatMessages}
        chatSources={chatSources}
        chatInput={chatInput}
        chatLoading={chatLoading}
        chatError={chatError}
        onChatInputChange={setChatInput}
        onChatSubmit={handleChatSubmit}
      />
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
        <div className="welcome-title">ASISTENT &nbsp; VIRTUAL</div>

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
              Logare
            </button>
            <button
              type="button"
              className={`tab-btn ${registerActive ? "active" : ""}`}
              onClick={() => switchMode("signup")}
            >
              Înregistrare
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
                </div>

                <button
                  type="submit"
                  className={`submit-btn ${isSubmittingAuth ? "loading" : ""}`}
                  disabled={isSubmittingAuth}
                >
                  Loghează-te
                </button>

                {formErr ? (
                  <div className="error-msg" style={{ marginTop: 10 }}>
                    {formErr}
                  </div>
                ) : null}
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
                  Creează cont
                </button>

                {formErr ? (
                  <div className="error-msg" style={{ marginTop: 10 }}>
                    {formErr}
                  </div>
                ) : null}
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
