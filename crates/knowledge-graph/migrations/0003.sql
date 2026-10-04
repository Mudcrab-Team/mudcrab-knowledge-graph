-- Preserve deterministic claim-ID order without sorting an entire high-degree node.
CREATE INDEX assertion_outgoing_ordered ON assertions(subject_id,id);
CREATE INDEX assertion_incoming_ordered ON assertions(object_id,id);
PRAGMA user_version = 3;
