# PAVE Router

`crates/pave-router` owns the Rust-native PAVE representation used by autonomous Marvis routing.

## Public Types

| Type | Purpose |
| --- | --- |
| `PaveVector` | Named `f32` vector dimensions with dot, norm, and cosine helpers. |
| `ToolAccess` | Approval-style capability flags for an agent profile. |
| `AgentProfile` | Runtime agent identity: shared model name, skill ids, MCP server ids, fallback instruction, tool allowlist, approval defaults, and PAVE vector. |
| `TaskCandidate` | LLM-segmented task with optional selected agent id, evidence, risk, desired tools, and task PAVE vector. |
| `RouteDecision` | Selected task/profile pair with internal route metrics and explanation. |
| `Router` | Matches task candidates against profiles and returns the best compatible route. |

## Behavior

Vectors are sparse maps. Cosine similarity is calculated over shared dimensions and returns `0.0` for zero vectors. Current autonomy asks the LLM segmenter to choose an agent id first; the router validates that the selected agent can support the task and uses PAVE matching only as a fallback for task payloads without an agent id.

The crate has no model or VSCode dependency. It is used by `src/autonomy.rs`; VSCode routing profiles are generated in Rust by `src/skill_mcp.rs` from discovered skills, declared tool functions, MCP dependencies, and built-in toolsets.
