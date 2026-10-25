---
schema: "agent-memory/v1"
project-id: "add009c6-d31e-49a6-a372-b8cab316d7bb"
router-updated: "2026-10-25T01:48:47.778230+00:00"
canonical-remote: "repo://github.com/robertelee78/agent-memory"
---
# Project memory

Router time describes this map, not every file's revision.

## Routes

| Path | Purpose | Scope | Lifecycle | Read when |
| --- | --- | --- | --- | --- |
| [projectbrief.md](<projectbrief.md>) | Project purpose and authority boundaries | repository | active | At session start to identify this project |
| [productContext.md](<productContext.md>) | Users, workflow and scope exclusions | repository | active | When deciding product scope or comparing requirements |
| [activeContext.md](<activeContext.md>) | Unresolved adoption and distribution question | repository | active | When working on repository adoption or installer distribution |
| [systemPatterns.md](<systemPatterns.md>) | Bank structure and Git/semantic workflow | repository | active | When changing memory design, capture or sharing |
| [techContext.md](<techContext.md>) | Inspected runtime, CLI and artifact behavior | repository | active | When working on installation, CLI or artifact integrity |
| [progress.md](<progress.md>) | Release evidence and amended roadmap | repository | active | When checking delivery claims or choosing milestone scope |

## Omitted contexts

None.
