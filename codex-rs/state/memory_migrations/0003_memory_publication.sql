-- Proposal only: one publication pointer per root within this version/scope database.
ALTER TABLE jobs ADD COLUMN publication_root_identity TEXT;
CREATE TABLE memory_publication (
    root_identity TEXT PRIMARY KEY NOT NULL,
    generation_id TEXT NOT NULL,
    notes_digest TEXT NOT NULL,
    artifact_manifest_digest TEXT NOT NULL,
    provenance TEXT NOT NULL CHECK (provenance IN ('legacy_import', 'consolidated')),
    legacy_empty INTEGER NOT NULL CHECK (legacy_empty IN (0, 1) AND (legacy_empty = 0 OR provenance = 'legacy_import'))
);
