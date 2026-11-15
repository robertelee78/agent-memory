---
schema: "agent-memory/v1"
project-id: "add009c6-d31e-49a6-a372-b8cab316d7bb"
router-updated: "2026-11-15T21:00:33.950695+00:00"
canonical-remote: "repo://github.com/robertelee78/agent-memory"
---
# Project memory

Router time describes this map, not every file's revision.

## Routes

| Path | Purpose | Scope | Lifecycle | Read when |
| --- | --- | --- | --- | --- |
| [projectbrief.md](<projectbrief.md>) | What agent-memory is and which documents are authoritative | repository | active | At session start to identify this project |
| [productContext.md](<productContext.md>) | Users, workflow and scope exclusions | repository | active | When deciding product scope or comparing requirements |
| [activeContext.md](<activeContext.md>) | Current focus, working rules and next steps | repository | active | When continuing current work |
| [systemPatterns.md](<systemPatterns.md>) | Bank shape, agent workflow, sharing and install patterns | repository | active | When changing memory design, sharing or installation |
| [techContext.md](<techContext.md>) | Rust stack, commands, build and the qa.sh quality gate | repository | active | When building, testing or changing am |
| [progress.md](<progress.md>) | What works, what remains, known limits | repository | active | When choosing what to do next |
| [topics/b309ea3e-8496-435e-b980-ee90afb9e5ab.md](<topics/b309ea3e-8496-435e-b980-ee90afb9e5ab.md>) | No proof, seal or re-verification machinery; no unit tests | repository | active | Before proposing a requirement, safeguard or test suite |
| [topics/829b92b6-6926-4e3e-801e-e51f88b62b57.md](<topics/829b92b6-6926-4e3e-801e-e51f88b62b57.md>) | Operator test-duration and execution preferences for release qualification | repository | active | When designing or running project qualification checks |
| [history/events/c76fd836-d207-4ebb-8450-8827baf0df6a.md](<history/events/c76fd836-d207-4ebb-8450-8827baf0df6a.md>) | History of release qualification guidance | repository | active | When reviewing corrections to qualification guidance |
| [topics/a7a752e7-b981-41b7-95be-f0f37de1e3e3.md](<topics/a7a752e7-b981-41b7-95be-f0f37de1e3e3.md>) | Resolve the actual OpenCode executable for harness tests | repository | active | When testing OpenCode integration |
| [history/events/f439f01a-ddcf-4833-8dc3-bff8f08b10ef.md](<history/events/f439f01a-ddcf-4833-8dc3-bff8f08b10ef.md>) | Admission provenance event | repository | active | When inspecting the admission history |

## Omitted contexts

None.

## Superseded lessons

Five lessons about removed machinery (per-turn sources, checkout identity, the
Python-era installer and caches) are kept in `topics/` with
`status: "superseded"` and are no longer routed.
