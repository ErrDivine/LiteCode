# PAVE Router

`crates/pave-router` owns the Rust-native PAVE representation used by autonomous Marvis routing.

## Public Types

| Type | Purpose |
| --- | --- |
| `PaveVector` | Named `f32` vector dimensions with dot, norm, and cosine helpers. |
| `ToolAccess` | Approval-style capability flags for an agent profile. |
| `AgentProfile` | Model name, skill ids, MCP server ids, fallback skill prompt, tool allowlist, approval defaults, and PAVE vector. |
| `TaskCandidate` | LLM-segmented task with evidence, risk, desired tools, confidence, and task PAVE vector. |
| `RouteDecision` | Selected task/profile pair with cosine score, final score, and explanation. |
| `Router` | Scores task candidates against profiles and returns the best route above threshold. |

## Behavior

Vectors are sparse maps. Cosine similarity is calculated over shared dimensions and returns `0.0` for zero vectors. Routing adds small tool-match bonuses and subtracts penalties for unavailable desired tools or high-risk tasks assigned to non-risky profiles.

The crate has no model or VSCode dependency. It is used by `src/autonomy.rs`, while profile values are sent from VSCode settings and defaulted in Rust when no valid profiles are configured.
