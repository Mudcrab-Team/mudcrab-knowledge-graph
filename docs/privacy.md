# Keep research local; export deliberately

Store real research outside this checkout. Do not commit databases, WAL/SHM files, parse caches, local root aliases, executables, game assets, captures, decompilation, memory dumps, credentials or machine-local logs. `.gitignore` is a convenience, not a substitute for inspecting staged files.

Visibility (`public`, `reviewed`, `restricted`), redistribution rights, lineage and review state are separate. Operators classify origins; extraction success does not grant rights. `ingest source` currently declares **public** source with license/redistribution defaults. For private code, use a correctly classified registry or bundle and a separate restricted store. Native/capture/material real inputs default restricted; use their `synthetic` option only for original inputs you may redistribute.

Source imports do not execute project code. Native export opens a prepared analyzed program read-only. Capture export replays a selected existing capture locally; it can require a compatible GPU/driver. Neither step is network acquisition or permission to forward output to a model.

## Fresh public export

```sh
/path/to/mudcrab-kg --root /external/working-store --context FULL_CONTEXT_ID \
  export --policy public --out /new/external/public-research
/path/to/mudcrab-kg --root /new/external/public-research --context public coverage
```

The destination must be new. Export constructs new filtered snapshots/IDs, copies only permitted evidence closure, rebuilds FTS and preserves current review decisions and attribution in `NOTICE.json`. Working-store IDs differ from exported IDs; pinned revisions and artifact locators remain verifiable. Restricted counts are not disclosed.

The exporter never copies the working database, its free pages/WAL/SHM, parse cache or `local-roots.json`. It removes claims whose endpoints or evidence are excluded. Inspect the output and its notices before sharing. Do not use a copied working SQLite file as a public artifact.

MCP's default launch policy is public. `research-local` additionally requires `--acknowledge-client-export`: model clients may send responses off-machine. The server cannot enforce what the client does with returned bytes. SQLite filtering is neither encryption nor multi-user isolation. Use separate OS identities/processes/databases for stronger researcher/implementer separation.

The automated source/privacy smoke includes an original restricted marker, then scans every exported file and checks that FTS/source cache do not retain it. This verifies the fixture's filtering, not every operator classification or possible private dataset. CI stores no research artifacts.
