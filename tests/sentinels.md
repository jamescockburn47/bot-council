# Sentinel Inventory

GENERATED FILE — do not edit by hand. Regenerate with:
`cargo test --test sentinels_test regen_inventory -- --ignored`
(a test fails when this file is stale).

A sentinel is a named runtime invariant. Violations surface as data
and warn-level logs with the stable ID — never panics, never blocked
output.

| ID | Invariant |
|---|---|
| EXT-001 | Extraction provenance source is one of: authored, extracted, extraction_failed. |
| EXT-002 | source == extracted implies a quote that verifies as a substring of the raw response. |
| SYN-001 | A successful crux selection yields exactly one is_crux issue in the artifact. |
| SYN-002 | meta_observations starts with "Conclusion:". |
| ING-001 | Ingest is total: any response body yields a stored result with a closed-set kind, never a dispatch error. |
