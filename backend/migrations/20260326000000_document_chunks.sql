CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE document_chunks (
  id uuid PRIMARY KEY,
  document_id uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  text_content text NOT NULL,
  embedding vector(1024) NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX document_chunks_document_id_idx
  ON document_chunks (document_id);

CREATE INDEX document_chunks_embedding_idx
  ON document_chunks USING hnsw (embedding vector_l2_ops);
