# Skill And MCP Runtime

`src/skill_mcp.rs` resolves routed agent profiles, while `src/skills.rs` owns Codex-style skill package loading. Skills and tools are separate layers: skills are archived instruction packages, local tool functions and MCP tools are the execution layer.

## Skills

The registry loads:

- bundled system skill packages: `rust-diagnostic-repair`, `test-failure-triage`, and `repo-explainer`
- workspace skills under `.marvis/skills/**/SKILL.md`
- Codex-compatible workspace skills under `.agents/skills/**/SKILL.md`

Bundled system skills are real packages with `SKILL.md`, `agents/openai.yaml`, `references/`, and `scripts/`. At runtime Marvis materializes them under `.lite-code/skills/.system` and loads them through the same package loader used for workspace skills.

Each `SKILL.md` must start with YAML-style frontmatter containing at least `name` and `description`. An optional `id` overrides the normalized skill id. Skill packages can include:

- `references/`: read-only Markdown or text references
- `scripts/`: script utilities that can run only when risky-shell approval is present
- `assets/`: package assets such as icons or templates

Optional metadata can live at `agents/openai.json` or `agents/openai.yaml` next to the skill file. Marvis reads:

- `dependencies.tools` entries with `type: local`, `tool`, `local_tool`, or `function` as local tool dependencies
- `dependencies.tools` entries with `type: mcp`, `value`, `transport: stdio`, `command`, and optional `args`
- legacy `capabilities` as local tool dependencies

Agent profiles name skills through `skills: ["skill-id"]`. The active local tool list is capped by both the agent profile allowlist and the selected skill's local tool dependencies. Selected skill bodies are injected into the system prompt, and equipped agents can use `list_skills`, `list_skill_resources`, and `read_skill_resource` to inspect package resources.

## MCP

The MCP runtime reads stdio server config from `.marvis/mcp.json` and `.mcp.json`.

```json
{
  "mcpServers": {
    "local-docs": {
      "transport": "stdio",
      "command": "uvx",
      "args": ["mcp-server-docs"],
      "enabled": true,
      "enabled_tools": []
    }
  }
}
```

Before an agent turn starts, Marvis initializes each selected MCP server, sends `tools/list`, and exposes only discovered tools. Because stdio MCP startup launches a local process, selected MCP servers require shell approval for that accepted turn. Tool names are qualified as `mcp__server__tool` and capped to the Responses API name length. Tool calls go back through the same stdio JSON-RPC path with `tools/call`.

Missing, disabled, failing, or tool-less required MCP servers fail closed for selected agents. Marvis does not synthesize MCP tools.
