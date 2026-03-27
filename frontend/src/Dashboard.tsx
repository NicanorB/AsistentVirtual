import type { ChangeEvent, FormEvent } from "react";
import type { ChatMessage, ChatSourceItem, DocumentRow } from "./types/auth";

type DashboardProps = {
  username?: string;
  accessToken: string;
  isRefreshing: boolean;
  docsLoading: boolean;
  docsError: string;
  documents: DocumentRow[];
  onLogout: () => void;
  uploadingDocument: boolean;
  onUploadDocument: (file: File | null) => void | Promise<void>;
  chatMessages: ChatMessage[];
  chatSources: ChatSourceItem[];
  chatInput: string;
  chatLoading: boolean;
  chatError: string;
  onChatInputChange: (value: string) => void;
  onChatSubmit: (message: string) => void | Promise<void>;
};

export default function Dashboard({
  username,
  isRefreshing,
  docsLoading,
  docsError,
  documents,
  onLogout,
  uploadingDocument,
  onUploadDocument,
  chatMessages,
  chatSources,
  chatInput,
  chatLoading,
  chatError,
  onChatInputChange,
  onChatSubmit,
}: DashboardProps) {
  const sub = username ?? null;

  const handleFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    await onUploadDocument(file);
    event.target.value = "";
  };

  const handleChatSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    await onChatSubmit(chatInput);
  };

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
            <div className="dashboard-title">DASHBOARD</div>
            <div className="dashboard-badge">{sub ? sub : "authenticated"}</div>
          </div>

          <div className="dashboard-actions">
            <label className="dash-btn" aria-disabled={uploadingDocument}>
              <input
                type="file"
                accept=".pdf,.docx,.txt,application/pdf,application/vnd.openxmlformats-officedocument.wordprocessingml.document,text/plain"
                onChange={handleFileChange}
                disabled={uploadingDocument || isRefreshing}
                style={{ display: "none" }}
              />
              {uploadingDocument ? "Uploading..." : "Upload Document"}
            </label>
            <button
              className="dash-btn danger"
              type="button"
              onClick={onLogout}
              disabled={isRefreshing || uploadingDocument}
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
                {uploadingDocument
                  ? "Uploading document…"
                  : docsLoading
                    ? "Loading…"
                    : docsError
                      ? "Unable to fetch documents."
                      : "Accepted file types: PDF, DOCX, TXT."}
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
                          <div className="document-meta">file: {doc.file}</div>
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

            <div className="chat-panel">
              <div>
                <div className="dashboard-section-title">Assistant Chat</div>
                <div className="dashboard-note">
                  Ask questions about your uploaded documents.
                </div>
              </div>

              <div className="chat-message-list">
                {chatMessages.length ? (
                  chatMessages.map((message) => (
                    <div
                      className={`chat-message ${message.role}`}
                      key={message.id}
                    >
                      <div className="chat-message-role">
                        {message.role === "user" ? "You" : "Assistant"}
                      </div>
                      <div className="chat-message-content">
                        {message.content || (chatLoading ? "Thinking…" : "")}
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="dashboard-note">
                    Start a conversation to query your document context.
                  </div>
                )}
              </div>

              {chatError ? (
                <div className="dashboard-error">{chatError}</div>
              ) : null}

              <form onSubmit={handleChatSubmit} className="chat-form">
                <textarea
                  className="chat-input"
                  value={chatInput}
                  onChange={(event) => onChatInputChange(event.target.value)}
                  placeholder="Ask the assistant about your documents"
                  disabled={chatLoading || isRefreshing}
                />

                <div
                  className="dashboard-actions"
                  style={{ justifyContent: "space-between" }}
                >
                  <div className="dashboard-note">
                    {chatLoading
                      ? "Streaming response…"
                      : "Responses use retrieved document context."}
                  </div>

                  <button
                    className="dash-btn"
                    type="submit"
                    disabled={
                      chatLoading || isRefreshing || !chatInput.trim().length
                    }
                  >
                    {chatLoading ? "Sending..." : "Send"}
                  </button>
                </div>
              </form>

              {chatSources.length ? (
                <div>
                  <div className="dashboard-section-title">Sources</div>
                  <div className="chat-sources">
                    {chatSources.map((source, index) => (
                      <div
                        className="document-row"
                        key={`${source.document}-${index}`}
                      >
                        <div className="document-main">
                          <div
                            className="document-title"
                            title={source.document}
                          >
                            {source.document}
                          </div>
                          <div className="chat-source-snippet">
                            {source.text_snippet}
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
