CREATE TABLE documents (
  id uuid PRIMARY KEY,
  user_id uuid NOT NULL REFERENCES users(id),
  title text NOT NULL,
  file text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  processed boolean NOT NULL DEFAULT false
);
