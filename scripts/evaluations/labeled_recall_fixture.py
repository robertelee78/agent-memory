"""Fixed synthetic project-memory quality labels; no oracle query expansion.

Fixture meanings are authored ground truth, not agent semantic judgments. Corpus
sizes count distinct identities (a correction may have two canonical versions).
"""
from __future__ import annotations
import copy
import hashlib
import json
import posixpath
import uuid

PID = "7eea1199-8123-4989-8cde-012d3361c8ee"
STAMP = "2026-10-25T08:00:00Z"
CONTEXTS = ["projectbrief.md", "productContext.md", "activeContext.md", "systemPatterns.md", "techContext.md", "progress.md"]


def identity(label):
    return str(uuid.UUID(bytes=hashlib.sha256(("quality-fixture/" + label).encode()).digest()[:16], version=4))


def header(value):
    return "---\n" + "".join(k + ": " + json.dumps(v) + "\n" for k, v in value.items()) + "---\n"


def record(template, label, body, **updates):
    m = copy.deepcopy(template)
    m.update({"id": identity(label), "project-id": PID, "kind": "semantic-fact", "status": "active", "scope": "repository", "conditions": {}, "trust": "reviewed", "visibility": "visible", "recorded-at": STAMP, "updated": STAMP, "admission-reason": "Authored synthetic evaluation assertion; not real admission evidence"})
    m["origin"].update({"developer-id": identity("developer-" + str(int(identity(label).replace("-", ""), 16) % 2)), "harness": "custom-fixture", "session": "fixed-quality-labels", "code-state": "not-code"})
    m["evidence"] = ["fixture://quality/" + label]
    m.update(updates)
    return {"label": label, "metadata": m, "body": body, "path": "topics/records/" + label + ".md"}


def cases(rows):
    by = {r["label"]: r for r in rows}
    def item(name, query, expected=(), *, reason, **options):
        request = {"project_id": PID, "query": query, "ids": [], "paths": [], "context": {}, "kinds": ["semantic-fact"], "minimum_trust": "reviewed", "limit": 16, "include_context": False, **options}
        return {"case": name, "request": request, "relevant_paths": [by[label]["path"] if label in by else label for label in expected], "rationale": reason}
    return [
        item("exact-id", "irrelevant words", ["retry-limit"], ids=[by["retry-limit"]["metadata"]["id"]], reason="Exact stable identity still receives all applicability/currentness gates."),
        item("exact-path", "", ["retry-limit"], paths=[by["retry-limit"]["path"]], reason="Exact canonical path returns current qualified source."),
        item("literal-multiple-terms", "retry transient", ["retry-limit"], reason="Both literal terms distinguish the retry observation from realistic distractors."),
        item("shared-terms-paraphrase", "retry requests", ["retry-limit"], reason="An ordinary reworded question retains shared domain words."),
        item("no-shared-terms-paraphrase", "How often may a temporary network error be attempted again?", ["retry-limit"], reason="Known natural-language lexical gap, retained as a miss; no fixture-aware expansion is fed to the reader."),
        item("two-relevant", "pagination cursor", ["cursor-empty", "cursor-repeated"], reason="Both separately evidenced edge cases are relevant."),
        item("inapplicable-component", "ledger retention", [], context={"component": "src/api"}, reason="Ledger-only guidance cannot become API guidance."),
        item("unknown-prerequisite", "delta resumption", [], reason="Prerequisite not observed; do not infer it from the query."),
        item("known-prerequisite", "delta resumption", ["conditional-resume"], context={"prerequisite": "receipt-verified"}, reason="Actual request supplies the declared non-Git prerequisite."),
        item("working-vs-head", "dispatch limit", ["head-dispatch"], reason="Record explicitly concerns main HEAD and remains labeled; reader must not assert working bytes verified. The apparatus stages a different source value."),
        item("stronger-correction", "legacytimeout", [], reason="Version 2 corrects version 1 even though replacement wording misses this query."),
        item("hidden-replacement", "legacycache", [], reason="Hidden qualified replacement still invalidates visible obsolete guidance."),
        item("unsafe-replacement", "legacyquota", [], reason="Unsafe replacement remains withheld and cannot resurrect its old claim."),
        item("expired", "sunset endpoint", [], reason="Active metadata with an expired valid-until is not current."),
        item("revoked", "withdrawn fallback", [], reason="Revoked metadata and route are not current."),
        item("superseded-state", "retired protocol", [], reason="Explicit superseded state remains historical."),
        item("distinct-conditions", "sandbox retention", ["retention-dev"], context={"environment": "development"}, reason="Development and production conditions must stay distinct; only the development claim qualifies."),
        item("duplicate-evidence", "idempotency receipt", ["receipt-a", "receipt-b"], reason="Both candidates carry different provenance; query recall must not erase either. Semantic deduplication is separately governed."),
        item("cross-file-contradiction", "atrium wall", ["wall-blue", "wall-yellow"], reason="Both claims must remain available for detecting-agent clarification, with source labels. Reader is not a semantic conflict detector."),
        item("orientation-and-typed", "deployment queue", ["deployment-queue", "projectbrief.md"], include_context=True, reason="Orientation is separate, trust null; typed source retains current-record label."),
        item("unsafe-source", "sensitivefixture", [], reason="Entire secret-bearing source is withheld without leaking its body or path."),
        item("no-match", "nonexistentquasar", [], reason="Valid bank with no relevant current source returns successful empty selection."),
        item("unknown-session", "resumable checkpoint", [], reason="Unknown required session must not qualify."),
    ]


def corpus(template, count):
    rows = []
    def add(label, body, **updates):
        value = record(template, label, body, **updates); rows.append(value); return value
    add("retry-limit", "# Bounded retries\n\nRetry transient requests three times with exponential backoff. Preserve the request identifier across attempts; a permanent validation failure is returned immediately.\n")
    add("cursor-empty", "# Pagination cursor: empty page\n\nAn empty page with a nonempty pagination cursor is not the end of the scan. Continue using the returned cursor until the server omits it.\n")
    add("cursor-repeated", "# Pagination cursor: repeated token\n\nDetect a repeated pagination cursor and stop the scan with a diagnostic, preserving the last completed page. This avoids a loop while retaining evidence.\n")
    add("ledger-only", "# Ledger retention\n\nLedger retention is seven years under the ledger component's reconciliation policy. The API component has a separate lifecycle.\n", scope="component:src/ledger")
    add("conditional-resume", "# Delta resumption\n\nDelta resumption may reuse a verified receipt after the source inventory agrees. An unverified receipt supplies no recovery authority.\n", conditions={"prerequisite": "receipt-verified"})
    add("head-dispatch", "# Dispatch limit at inspected HEAD\n\nOn the inspected main revision, dispatch limit is three. This observation does not certify staged or working code. Inspect current source before changing dispatch behavior.\n", conditions={"branch": "main"})
    old = add("timeout-v1", "# Legacytimeout\n\nLegacytimeout is thirty seconds. This historical assertion has a newer exact correction.\n")
    new = add("timeout-v2", "# Revised deadline\n\nThe confirmed request deadline is sixty seconds after the revised acceptance probe.\n", version=2, corrects=[{"id": old["metadata"]["id"], "version": 1}]); new["metadata"]["id"] = old["metadata"]["id"]
    old = add("cache-v1", "# Legacycache\n\nLegacycache persists every stale response. A qualified hidden replacement invalidates this assertion.\n")
    add("cache-replacement", "# Cache policy revision\n\nDiscard stale responses under the current cache policy.\n", visibility="hidden", supersedes=[{"id": old["metadata"]["id"], "version": 1}])
    old = add("quota-v1", "# Legacyquota\n\nLegacyquota is unlimited. A qualified replacement invalidates this assertion.\n")
    add("quota-replacement", "# Quota policy revision\n\nThe confirmed quota is finite.\npassword=WITHHELD_REPLACEMENT_FIXTURE\n", supersedes=[{"id": old["metadata"]["id"], "version": 1}])
    add("expired-endpoint", "# Sunset endpoint\n\nThe sunset endpoint was reachable only during the retired compatibility window.\n", **{"valid-until": "2026-01-01T00:00:00Z"})
    add("revoked-fallback", "# Withdrawn fallback\n\nThe withdrawn fallback returned inconsistent identifiers.\n", status="revoked", **{"state-reason": "Fixed fixture records an explicit revocation"})
    add("superseded-protocol", "# Retired protocol\n\nRetired protocol details are historical.\n", status="superseded", **{"state-reason": "Fixed fixture records a replacement", "superseded-by": [{"id": identity("superseded-protocol"), "version": 2}]})
    add("retention-dev", "# Sandbox retention\n\nSandbox retention in development is one day to reduce fixture buildup.\n", conditions={"environment": "development"})
    add("retention-prod", "# Sandbox retention\n\nSandbox retention in production is thirty days under the audit contract.\n", conditions={"environment": "production"})
    add("receipt-a", "# Idempotency receipt\n\nThe idempotency receipt keeps the first successful response for a repeated request key. Evidence comes from the HTTP acceptance probe.\n")
    add("receipt-b", "# Idempotency receipt\n\nA repeated request key returns the first successful response recorded in its idempotency receipt. Evidence comes from the independent queue probe.\n")
    add("wall-blue", "# Atrium wall\n\nThe atrium wall color should be blue. The current design meeting supplied this claim; clarify it against the separate yellow claim.\n")
    add("wall-yellow", "# Atrium wall\n\nThe atrium wall color should be yellow. The current prototype review supplied this claim; clarify it against the separate blue claim.\n")
    add("deployment-queue", "# Deployment queue\n\nThe deployment queue preserves arrival order while a worker is unavailable. Retry a failed deployment using its original identifier.\n")
    add("unsafe-secret", "# Sensitivefixture\n\npassword=WITHHELD_SOURCE_FIXTURE\n")
    add("session-bound", "# Resumable checkpoint\n\nA resumable checkpoint describes only the named session's unfinished operation.\n", conditions={"session": "another-session"})
    # Fixed-domain distractors combine distinct components, concrete values and
    # conditions. A small long-tail exercises body length without repeating one
    # dummy line 50,000 times; no distractor uses special query identifiers.
    domains = [
        ("journal", "The journal segment for partition {n} is sealed after {v} entries. Its checksum is verified before rotation; a partial footer requires recovery from the preceding complete segment."),
        ("worker", "Worker shard {n} accepts at most {v} outstanding jobs. Admission stops when the shard drains; finished jobs preserve their original outcome identifier for reconciliation."),
        ("artifact", "Artifact family {n} uses generation {v}. Validate the manifest digest before unpacking and retain the source revision with the published result."),
        ("parser", "Parser dialect {n} accepts at most {v} nesting levels. A malformed delimiter returns a location diagnostic and preserves the original input for inspection."),
        ("socket", "Socket pool {n} retains {v} idle connections per origin. A peer close removes that connection before another request is assigned."),
        ("scheduler", "Scheduler lane {n} has a weight of {v}. Weight changes apply to new admissions and leave the ordering of already accepted jobs unchanged."),
        ("snapshot", "Snapshot family {n} is retained for {v} complete cycles. Removal checks all dependent readers and records the last referenced generation."),
        ("encoder", "Encoder profile {n} emits blocks of {v} units. A failed final checksum keeps the preceding artifact intact and reports the incomplete new generation."),
    ]
    distinct = len({r["metadata"]["id"] for r in rows})
    for n in range(count - distinct):
        domain, pattern = domains[n % len(domains)]
        body = "# " + domain.title() + " component " + str(n) + "\n\n" + pattern.format(n=n, v=2 + n % 127) + "\n"
        if n % 20 == 0:
            body += "\n" + ("Record the inspected component and the reproduction conditions before applying this observation elsewhere. A later implementation can change this behavior; source evidence takes precedence over the remembered summary. " * (1 + n % 7)) + "\n"
        add("distractor-" + str(n).zfill(6), body, conditions={"component": "src/" + domain + "/" + str(n % 31)} if n % 3 == 0 else {})
    def table(children, source):
        text = "| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n"
        for path, scope, state in children:
            link = posixpath.relpath(path, posixpath.dirname(source) or ".")
            text += "| [" + path + "](<" + link + ">) | Synthetic project evidence | " + scope + " | " + state + " | When reviewing this component |\n"
        return text
    files = {"projectbrief.md": "# Project orientation\n\nThe deployment queue belongs to this synthetic project. This prose supplies orientation only; check the separately evidenced typed queue lesson.\n"}
    for r in rows:
        files[r["path"]] = header(r["metadata"]) + r["body"]
    leaves = []
    for start in range(0, len(rows), 64):
        path = "topics/leaves/index-" + str(start // 64).zfill(4) + ".md"
        children = [(r["path"], r["metadata"]["scope"], r["metadata"]["status"]) for r in rows[start:start+64]]
        files[path] = header({"schema": "agent-memory/topic-index-v1", "project-id": PID, "updated": STAMP}) + "# Topic index\n\n## Routes\n\n" + table(children, path)
        leaves.append((path, "repository", "active"))
    groups = []
    for start in range(0, len(leaves), 64):
        path = "topics/groups/group-" + str(start // 64).zfill(3) + ".md"
        files[path] = header({"schema": "agent-memory/topic-index-v1", "project-id": PID, "updated": STAMP}) + "# Group index\n\n## Routes\n\n" + table(leaves[start:start+64], path)
        groups.append((path, "repository", "active"))
    files["MEMORY.md"] = header({"schema": "agent-memory/v1", "project-id": PID, "router-updated": STAMP}) + "# Project memory\n\n## Routes\n\n" + table([("projectbrief.md", "repository", "active"), *groups], "MEMORY.md") + "\n## Omitted contexts\n\n" + "".join("- `" + path + "`: No separate evidence in this synthetic fixture\n" for path in CONTEXTS[1:])
    assert len({r["metadata"]["id"] for r in rows}) == count
    return files, rows, cases(rows)
