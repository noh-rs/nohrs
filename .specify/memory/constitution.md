<!--
Sync Impact Report
- Version change: N/A → 1.0.0
- Modified principles:
	- Added: I. Performance & Responsiveness First
	- Added: II. Modular, Plugin-Ready Architecture
	- Added: III. Unified Interfaces (CLI/HTTP)
	- Added: IV. Security & Safety by Default
	- Added: V. Observability & Diagnostics
- Added sections:
	- Additional Constraints & Standards
	- Development Workflow & Quality Gates
- Removed sections: None
- Templates requiring updates:
	- ✅ .specify/templates/plan-template.md (aligned; no change required)
	- ✅ .specify/templates/spec-template.md (aligned; no change required)
	- ✅ .specify/templates/tasks-template.md (updated to reflect constitution test/observability guidance)
	- ⚠ .specify/templates/commands/ (not found; create and validate command docs later)
- Follow-up TODOs:
	- Define per-feature performance budgets in each plan/spec and revise defaults if needed.
	- Establish logging schema and redaction policy; document in developer guide.
	- Create `.specify/templates/commands/` set to standardize command docs.
-->

# Nohr Constitution

## Core Principles

### I. Performance & Responsiveness First

- The application MUST provide fast interactions for common operations.
- Default performance budgets (can be tightened per feature in plan/spec):
  - Local directory listing: p95 < 150 ms for typical folders; MUST lazy‑load for very large folders.
  - Incremental search/filtering: p95 < 100 ms per keystroke with indexing enabled.
  - S3/remote listings: p95 < 500 ms with caching; background prefetch where applicable.
- Large datasets MUST use lazy loading, pagination, and background indexing to avoid UI stalls.
- Performance regressions MUST be called out in PRs with a mitigation or accepted tradeoff rationale.

Rationale: The product’s core value is speed and smoothness across local and remote sources.

### II. Modular, Plugin-Ready Architecture

- Features MUST be implemented as decoupled libraries with clear contracts and minimal UI coupling.
- Public capabilities SHOULD expose stable extension points (e.g., WASM plugins) where appropriate.
- Each library MUST be independently testable and documented (purpose, inputs/outputs, error modes).
- Cross-cutting concerns (e.g., storage, logging) MUST live in shared libraries, not in UI layers.

Rationale: Modularity enables maintainability, parallel development, and third‑party extensions.

### III. Unified Interfaces (CLI/HTTP)

- Public operations MUST be invokable via CLI and, when needed, an HTTP API for automation.
- Text I/O contract: stdin/args → stdout; errors → stderr. JSON and human‑readable outputs MUST be supported.
- Commands SHOULD be idempotent where reasonable and MUST provide exit codes for automation.

Rationale: A unified interface supports scripting, integrations, and repeatable workflows.

### IV. Security & Safety by Default

- Secrets and credentials MUST be stored in macOS Keychain; no plain‑text secrets in config.
- The app MUST detect and guide required permissions (e.g., Full Disk Access) before accessing protected paths.
- Destructive actions MUST be reversible (trash/undo/history) and clearly communicated.
- External API communications MUST use authenticated/signed requests as applicable; sensitive data MUST be redacted in logs.

Rationale: Users trust their file tool with sensitive data; defaults must protect them.

### V. Observability & Diagnostics

- Structured logging MUST exist for core flows (file ops, indexing, transfers, plugins, network).
- In debug builds, feature‑level metrics SHOULD be captured to surface hotspots; production telemetry MUST be opt‑in.
- Errors MUST include sufficient context for reproduction without leaking sensitive data.

Rationale: Visibility shortens triage time and improves reliability.

## Additional Constraints & Standards

- Primary stack: Rust (async via tokio/rayon), UI via gpui.
- Core libraries: search (tantivy/ripgrep), storage (aws‑sdk‑rust/object_store), VCS (git2/gix), markdown (comrak/syntect), data (sled/SQLite), fs watch (notify).
- Platform: macOS first‑class. Keyboard and interaction models SHOULD align with VS Code and Terminal conventions.
- Previews: Images, PDF, text, Markdown MUST render inline where feasible.
- Performance budgets: Defaults defined above apply unless overridden in the feature plan/spec with measurements.
- Security: Keychain for secrets, permission prompts, and safe deletion policies are mandatory.

## Development Workflow & Quality Gates

- Documentation set per feature: plan.md (required), spec.md (required for user stories), research.md, data‑model.md, contracts/.
- Constitution Check gate (in plan.md): The plan MUST state how it meets Core Principles I–V and call out any justified exceptions.
- Independent deliverability: User stories in spec.md MUST be independently implementable and demo‑able.
- Testing policy:
  - Contract/integration tests are REQUIRED when exposing or changing public CLI/HTTP contracts.
  - Other tests MAY be requested by the feature spec; when requested, write tests first and ensure they initially fail.
- Code review: At least one reviewer MUST confirm constitution compliance, performance impact, and security notes.
- Logging: New features MUST include logging at appropriate levels and ensure no sensitive data is emitted.

## Governance

- Authority: This constitution governs engineering practices for Nohr and supersedes conflicting doc guidance.
- Amendments: Changes are proposed via PR with:
  - Rationale and impact analysis,
  - Migration/communication plan if behavior expectations change,
  - Version bump per policy below.
- Versioning policy (semantic):
  - MAJOR: Backward‑incompatible governance changes or principle removals/redefinitions.
  - MINOR: New principle/section added or materially expanded guidance.
  - PATCH: Clarifications, wording, typo fixes, non‑semantic refinements.
- Compliance: PR templates and reviewers MUST verify constitution gates. Exceptions require explicit, time‑boxed justification in the plan.

**Version**: 1.0.0 | **Ratified**: 2025-11-06 | **Last Amended**: 2025-11-06
