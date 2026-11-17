-- The starter migration. It proves the path from this folder to a running
-- database, and gives the first real table something to follow.

CREATE TABLE bootstrap_notes (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    note       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO bootstrap_notes (note) VALUES ('The database is ready.');
