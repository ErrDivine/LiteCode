# Skill And MCP Runtime

`src/skill_mcp.rs` generates and resolves routed agent identities, while `src/skills.rs` owns Codex-style skill package loading. Skills and tools are separate layers: skills are archived instruction packages, local tool functions and MCP tools are the execution layer.

## Skills

The registry loads:

- bundled system skill packages from `skills/system/**/SKILL.md`, including native Marvis skills, imported Anthropic skills, and imported local Codex workspace skills
- workspace skills under `.marvis/skills/**/SKILL.md`
- Codex-compatible workspace skills under `.agents/skills/**/SKILL.md`

Bundled system skills are real packages with `SKILL.md`, optional `agents/openai.yaml`, and package resources. A build script generates a binary-safe catalog from `skills/system`, so text resources, scripts, images, fonts, PDFs, and archives can ship without a manual Rust manifest. At runtime Marvis materializes them under `.lite-code/skills/.system` and loads them through the same package loader used for workspace skills.

Each `SKILL.md` must start with YAML-style frontmatter containing at least `name` and `description`. An optional `id` overrides the normalized skill id. Skill packages can include:

- any non-hidden package files as resources, excluding `SKILL.md` and `agents/openai.{yaml,json}` metadata
- `scripts/`: script utilities that can run only when risky-shell approval is present
- `assets/`, fonts, images, PDFs, and archives as asset resources
- `references/`, `reference/`, `examples/`, `templates/`, `themes/`, root Markdown guides, and similar files as reference resources

Optional metadata can live at `agents/openai.json` or `agents/openai.yaml` next to the skill file. Marvis reads:

- `dependencies.tools` entries with `type: local`, `tool`, `local_tool`, or `function` as local tool dependencies
- `dependencies.tools` entries with `type: mcp`, `value`, `transport: stdio`, `command`, and optional `args`
- legacy `capabilities` as local tool dependencies

Each implicitly invokable skill becomes a same-model agent identity. The generated agent id is the skill id, the label comes from skill interface metadata when present, and the tool allowlist comes from declared local tool dependencies. Imported bundled skills include `agents/openai.yaml` metadata so they register as useful agents instead of read-only placeholders. Skills without explicit tool metadata still receive a read-only inspection toolset. Marvis also generates built-in toolset agents for read-only inspection, verification, and focused workspace patches.

The active local tool list is capped by runtime approval and the generated agent tool allowlist. Selected skill bodies are injected into the system prompt, and equipped agents can use `list_skills`, `list_skill_resources`, and `read_skill_resource` to inspect package resources. A skill can set `policy.allow_implicit_invocation: false` in `agents/openai.yaml` or `agents/openai.json` to stay available for explicit future use without becoming an autonomous routing target.

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
