# Agents querying the graph

Build with `--features mcp`. Start a dedicated stdio server for one existing store and context:

```sh
/path/to/mudcrab-kg --root /external/research-store \
  --context FULL_CONTEXT_ID serve --transport stdio --policy public
```

For a model client that accepts the common `mcpServers` configuration, use absolute paths:

```json
{
  "mcpServers": {
    "mudcrab-kg": {
      "command": "/absolute/path/to/mudcrab-kg",
      "args": ["--root", "/external/research-store", "--context", "FULL_CONTEXT_ID", "serve", "--transport", "stdio", "--policy", "public"]
    }
  }
}
```

Replace the context placeholder with a real full ID. Client configuration formats vary; the command and arguments are the interface. The server opens the existing store read-only and writes only protocol messages to stdout. Context and policy are fixed at launch, even if another process later moves the context name.

| Tool | Purpose |
|---|---|
| `kg.search` | Bounded lexical search with kind/source filters |
| `kg.node` | One contextualized node and citations |
| `kg.neighbors` | Bounded typed adjacency with pagination |
| `kg.trace` | Concept/material traversal and explicit missing stages |
| `kg.evidence` | Verified, byte-bounded artifact excerpt |
| `kg.context` | Bounded context packet from seed nodes |
| `kg.coverage` | Declared scope, capabilities and extraction status |

Generated schemas reject extra fields. Tools accept no SQL, filesystem paths, context override, policy override or review operation. Query envelopes are the same as CLI results. A public server cannot raise its policy. A `research-local` launch additionally requires `--acknowledge-client-export`, since the client may transmit returned text to its model provider. Use a physically separate public store for implementation agents where separation matters.

## Retrieval discipline

Start with coverage, then bounded search. Retrieve nodes and evidence before stating relationships. Cite immutable context, exact revision/hash, locator and full evidence ID. Keep admitted evidence, proposed mappings and missing links distinct. A review decision does not erase a relationship's manual/hypothesis origin.

For traces, inspect each assertion's provenance and current state. Do not infer a direct call from a SCIP reference or a source/native/GPU correspondence from matching names. `not_indexed`, extraction failures and indexed-but-unresolved relationships need different next actions. A lexical no-match result does not prove absence in the original engine.

The default packet budget is 64 KiB and the hard maximum is 256 KiB. Budgets include evidence and envelope metadata, not just claims. Truncated results are incomplete selections; follow documented cursors or narrower queries. Hypotheses occupy a separate packet section. An estimated token count is only an estimate.

Agents can prepare [proposals](research-workflow.md) outside Git. Import and review are explicit local CLI actions; MCP cannot write or execute suggestions. Do not admit your own semantic reasoning as extracted behavior. Inspect rights before forwarding research to any external model.

A real transport check against an existing source context:

```sh
python3 scripts/knowledge-graph/smoke_mcp.py \
  --binary "$PWD/target/debug/mudcrab-kg" --root /external/source-smoke/graph \
  --context source --query Store --out /external/mcp-result.json
```

This performs initialize/discover/call, checks all seven schemas and read-only annotations, compares CLI/MCP results exactly and rejects four SQL/path/context/policy escalation attempts. It does not call a model API.

Agents editing this repository must also follow [AGENTS.md](../AGENTS.md).
