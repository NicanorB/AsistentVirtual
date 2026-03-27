import type { ChangeEvent } from "react";
import type { DocumentRow } from "./types/auth";

type DashboardProps = {
  accessToken: string;
  expiresInSeconds: number;
  isRefreshing: boolean;
  docsLoading: boolean;
  docsError: string;
  documents: DocumentRow[];
  onLogout: () => void;
  decodeJwtSub: (token: string) => string | null;
  uploadingDocument: boolean;
  onUploadDocument: (file: File | null) => void | Promise<void>;
};

export default function Dashboard({
  accessToken,
  expiresInSeconds,
  isRefreshing,
  docsLoading,
  docsError,
  documents,
  onLogout,
  decodeJwtSub,
  uploadingDocument,
  onUploadDocument,
}: DashboardProps) {
  const sub = accessToken ? decodeJwtSub(accessToken) : null;

  const handleFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    await onUploadDocument(file);
    event.target.value = "";
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
            <div className="dashboard-title">MOCK DASHBOARD</div>
            <div className="dashboard-badge">
              {sub ? `user: ${sub}` : "authenticated"} · expires:{" "}
              {expiresInSeconds ? `${expiresInSeconds}s` : "-"}
            </div>
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
          </div>
        </div>
      </div>
    </div>
  );
}
