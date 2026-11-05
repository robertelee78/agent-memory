#!/usr/bin/env python3
"""Run bounded synthetic qualification against REAL endpoints; never start services.

Use managed_endpoints.py for serial explicitly approved owned llama.cpp launches.
Missing real endpoints are prerequisites, never replaced by mock compatibility.
"""
from __future__ import annotations
import argparse
import copy
import json
from pathlib import Path
import time
from common import (api_base, bind_sources, choose_model, cosine, create_run, digest,
                    embed, inventory, key_from, load, request, save, validate)

PROBES = ["Synthetic pagination retains a continuation token.",
          "Synthetic retry budgets stop after three attempts.",
          "Synthetic generation publication uses a complete candidate.",
          "Synthetic erased records cannot reappear from a stale build."]
DOCS = [
    ("retry", "Retry a temporary synthetic transport failure at most three times.", "pagination"),
    ("pagination", "Continue a synthetic paginated scan whenever a next cursor exists, even after an empty page.", "retry"),
    ("publication", "Publish a synthetic generation only after every required vector and graph node is complete.", "deletion"),
    ("deletion", "A synthetic privacy epoch fences stale builders before deleted content can be republished.", "publication"),
    ("orientation", "The synthetic compiler pipeline parses input before lowering its intermediate representation.", "retry"),
    ("unrelated", "The synthetic cafeteria menu rotates fruit on Thursdays.", "pagination"),
]
QUESTIONS = [("How many times may a transient failure be tried again?", "retry"),
             ("Does an empty page end a scan if continuation remains?", "pagination"),
             ("When is an unfinished graph safe to activate?", "publication"),
             ("What stops an old worker restoring removed guidance?", "deletion")]
PREFIX = "Instruct: Retrieve applicable saved project guidance for the question.\nQuery: "


def safe_answer(answer):
    # Synthetic-only responses are retained verbatim separately; compact summary has no vectors.
    b = answer["body"]
    return {k: v for k, v in answer.items() if k != "body"} | {
        "model": b.get("model"), "usage": b.get("usage"), "error": b.get("error"),
        "indices": [r.get("index") for r in b.get("data", []) if isinstance(r, dict)]}


def expect_refusal(call, reason):
    try:
        call()
    except ValueError as e:
        return {"status": "refused", "reason": str(e), "expected": reason,
                "matched": str(e) == reason, "exit": 1}
    return {"status": "unexpected-acceptance", "expected": reason, "matched": False}


def mutation_controls(actual, model, dimension):
    """Pure validator protocol simulation, using a real response as the starting point."""
    controls = {}
    for name, mutate, reason in [
        ("duplicate-index", lambda b: b["data"][1].update(index=b["data"][0]["index"]), "response-index-coverage"),
        ("missing-row", lambda b: b["data"].pop(), "response-count-mismatch"),
        ("extra-row", lambda b: b["data"].append(copy.deepcopy(b["data"][0])), "response-count-mismatch"),
        ("boolean-index", lambda b: b["data"][0].update(index=True), "response-index-coverage"),
        ("wrong-dimension", lambda b: b["data"][0]["embedding"].pop(), "response-dimension-mismatch"),
        ("nonfinite", lambda b: b["data"][0]["embedding"].__setitem__(0, float("nan")), "response-nonfinite-or-nonnumeric"),
        ("wrong-model", lambda b: b.update(model="synthetic-wrong-alias"), "response-model-mismatch"),
    ]:
        candidate = copy.deepcopy(actual)
        mutate(candidate["body"])
        controls[name] = expect_refusal(lambda: validate(candidate, model, 2, dimension), reason)
    return {"evidence_class": "protocol-simulation-not-provider-behavior", "controls": controls}


def qualify(root, config):
    endpoint = api_base(config["url"])
    key = key_from(config.get("key_file"))
    dim = config["dimension"]
    metadata = request(endpoint, "/models", key=key)
    save(root / "models-response.json", metadata)
    model, assurance = choose_model(metadata, config.get("model"))
    try:
        selected, _ = choose_model(metadata)
    except ValueError:
        selected = None  # Explicit models remain usable; ambiguity is never silently guessed.
    result = {"schema": "am255/actual-endpoint-controls-v1", "production_api_guarantee": False,
              "endpoint": endpoint, "model": model, "discovered_model": selected,
              "identity_assurance": assurance, "dimension": dim, "remote_retention": "not exercised; loopback synthetic only",
              "public_outcome": {"status": "qualified", "exit": 0, "mode": "scratch-compatibility"}}
    baseline, answer = embed(endpoint, model, PROBES[:2], dim, key)
    save(root / "baseline-response.json", answer)
    result["coverage"] = safe_answer(answer)
    result["protocol_simulation"] = mutation_controls(answer, model, dim)
    if not all(c["matched"] for c in result["protocol_simulation"]["controls"].values()):
        raise ValueError("Validator protocol-simulation control unexpectedly accepted")
    unknown = "am255-unknown-" + digest(PROBES)[:16]
    alias = request(endpoint, "/embeddings", {"input": PROBES[:1], "model": unknown}, key)
    save(root / "unknown-model-response.json", alias)
    result["unknown_alias"] = safe_answer(alias)
    result["unknown_alias"]["independent_metadata_refusal"] = expect_refusal(
        lambda: choose_model(metadata, unknown), "model-not-in-independent-metadata")
    result["unknown_alias"]["echo_is_not_weight_identity"] = True
    if alias["status"] == 200:
        try:
            v = validate(alias, unknown, 1, dim)[0]
            result["unknown_alias"]["cosine_to_known_alias"] = cosine(v, baseline[0])
        except ValueError as e:
            result["unknown_alias"]["validation_refusal"] = str(e)
    batch = []
    for n in (1, 4, 8):
        _, a = embed(endpoint, model, ["Synthetic batch row " + str(i) for i in range(n)], dim, key)
        save(root / ("batch-" + str(n) + "-response.json"), a)
        batch.append({"inputs": n, **safe_answer(a)})
    result["batch_controls"] = {"observed": batch, "max_qualified_inputs": 8,
        "scope": "small synthetic batches only; not provider maximum or throughput acceptance"}
    result["token_and_truncation"] = token_controls(root, config, model, key)
    result["probe_at_fill"] = probe_controls(root, endpoint, model, dim, key)
    result["own_vs_neighbor"] = text_controls(root, endpoint, model, dim, key)
    if key:
        unauthorized = request(endpoint, "/embeddings", {"model": model, "input": PROBES[:1]})
        save(root / "unauthorized-response.json", unauthorized)
        result["optional_key"] = {"authorized_embedding_http": answer["status"],
            "unauthorized_embedding_http": unauthorized["status"],
            "protected": unauthorized["status"] in (401, 403), "key_material_retained": False}
        if not result["optional_key"]["protected"]:
            raise ValueError("Configured key-protected actual endpoint accepted an unauthenticated embedding")
    else:
        result["optional_key"] = {"status": "prerequisite", "reason": "No key-protected real endpoint in this cell"}
    return result


def token_controls(root, config, model, key):
    context = config.get("context_tokens")
    tokenizer = config.get("tokenizer_url")
    if not context or not tokenizer:
        return {"assurance": "unverified", "status": "prerequisite",
                "reason": "Opaque endpoint lacks independently configured context/tokenizer; no no-truncation pass"}
    if type(context) is not int or not 32 <= context <= 32768:
        raise ValueError("Context bound outside bounded qualification range")
    short = "Synthetic complete input with an unmistakable terminal marker AM255_END."
    long = " synthetic" * (context + 64) + " AM255_DIFFERENT_END"
    counts = []
    for name, text in (("short", short), ("over-context", long)):
        a = request(tokenizer, "", {"content": text, "add_special": True}, key)
        save(root / (name + "-tokenizer-response.json"), a)
        tokens = a["body"].get("tokens")
        count = len(tokens) if a["status"] == 200 and isinstance(tokens, list) else None
        counts.append({"cell": name, "rendered_sha256": digest(text), "tokens": count})
    over = request(config["url"], "/embeddings", {"model": model, "input": [long]}, key)
    save(root / "over-context-response.json", over)
    short_answer = request(config["url"], "/embeddings", {"model": model, "input": [short]}, key)
    save(root / "short-usage-response.json", short_answer)
    prompt_tokens = short_answer["body"].get("usage", {}).get("prompt_tokens")
    exact_tokens = counts[0]["tokens"]
    difference = abs(prompt_tokens - exact_tokens) if type(prompt_tokens) is int and exact_tokens is not None else None
    bounded_refusal = counts[1]["tokens"] is not None and counts[1]["tokens"] > context and over["status"] in (400, 413, 422)
    ceiling = min(context, 8192)
    budget_text = " synthetic" * (ceiling // 2 + 64)
    budget_answer = request(tokenizer, "", {"content": budget_text, "add_special": True}, key)
    save(root / "aggregate-budget-tokenizer-response.json", budget_answer)
    budget_tokens = budget_answer["body"].get("tokens")
    per_input = len(budget_tokens) if budget_answer["status"] == 200 and isinstance(budget_tokens, list) else None
    def preflight(token_counts):
        if len(token_counts) > 8:
            raise ValueError("maintenance-batch-input-limit")
        if any(n > context for n in token_counts) or sum(token_counts) > ceiling:
            raise ValueError("maintenance-aggregate-token-limit")
    budget_controls = {"evidence_class": "local-preflight-policy-using-real-tokenizer-not-provider-maxima",
                      "max_inputs": 8, "aggregate_token_ceiling": ceiling, "embedding_requests_sent": 0}
    if exact_tokens is not None:
        budget_controls["nine_inputs"] = expect_refusal(lambda: preflight([exact_tokens] * 9), "maintenance-batch-input-limit")
    if per_input is not None:
        budget_controls["two_input_aggregate_tokens"] = 2 * per_input
        budget_controls["aggregate_limit"] = expect_refusal(lambda: preflight([per_input] * 2), "maintenance-aggregate-token-limit")
    else:
        budget_controls["status"] = "unverified-tokenizer-prerequisite"
    return {"configured_context_tokens": context, "token_counts": counts,
            "over_context": safe_answer(over), "over_context_explicitly_refused": bounded_refusal,
            "short_usage_token_difference": difference, "usage_diagnostic_tolerance_tokens": 2,
            "assurance": "bounded observed refusal; not proof against all silent truncation" if bounded_refusal else "unverified",
            "aggregate_maintenance_token_ceiling": min(context, 8192),
            "maintenance_preflight": budget_controls,
            "scope": "Tokenizer/native route is explicit and profile-specific, never guessed for BYO; no clipped input is sent"}


def probe_controls(root, endpoint, model, dim, key):
    measurements, vectors = [], []
    for i in range(3):
        v, a = embed(endpoint, model, PROBES, dim, key)
        vectors.append(v)
        measurements.append(safe_answer(a))
        save(root / ("probe-repeat-" + str(i) + "-response.json"), a)
    deviations = [{"repeat": i, "max_abs_coordinate": max(abs(x-y) for a,b in zip(vectors[0], vectors[i]) for x,y in zip(a,b)),
                   "max_cosine_distance": max(abs(1-cosine(a,b)) for a,b in zip(vectors[0], vectors[i]))} for i in (1,2)]
    _, fill = embed(endpoint, model, [x[1] for x in DOCS], dim, key)
    save(root / "fill-cost-response.json", fill)
    seconds = sum(x["seconds"] for x in measurements)/3
    return {"probe_spec_sha256": digest(PROBES), "inputs_per_fill": len(PROBES), "repetitions": measurements,
            "deviations": deviations, "diagnostic_tolerances": {"max_abs_coordinate": 1e-5, "max_cosine_distance": 1e-5},
            "within_diagnostic_tolerance": all(x["max_abs_coordinate"] <= 1e-5 and x["max_cosine_distance"] <= 1e-5 for x in deviations),
            "mean_probe_seconds": seconds, "six_document_fill_seconds": fill["seconds"],
            "probe_overhead_fraction_of_tiny_fill": seconds/fill["seconds"],
            "extra_prompt_tokens_per_fill": measurements[0].get("usage"),
            "identity_proof": False, "performance_acceptance": False,
            "scope": "Three immediate repeats measure only this cell; cross-restart/backend/platform tolerance remains a prerequisite"}


def text_controls(root, endpoint, model, dim, key):
    by_id = {key: body for key, body, _ in DOCS}
    result = {"fixture_sha256": digest(DOCS), "truth_sha256": digest(QUESTIONS),
              "ssg_pattern": "embed.rs unit_text and unit_text_related; adapted synthetic texts, not SSG renderer equivalence",
              "quality_pass": False, "cells": {}}
    query, qa = embed(endpoint, model, [PREFIX + q for q, _ in QUESTIONS], dim, key)
    save(root / "comparison-query-response.json", qa)
    for kind in ("own", "with-neighbor"):
        texts = [body if kind == "own" else body + "\nRelated title: " + by_id[neighbor] for _, body, neighbor in DOCS]
        values, a = embed(endpoint, model, texts, dim, key)
        save(root / (kind + "-response.json"), a)
        rows = []
        for (q, expected), v in zip(QUESTIONS, query):
            order = sorted(((cosine(v, w), key) for (key, _, _), w in zip(DOCS, values)), key=lambda x: (-x[0], x[1]))
            rows.append({"query_sha256": digest(q), "expected": expected, "ranking": order,
                         "reciprocal_rank": 1/(1 + next(i for i, (_, k) in enumerate(order) if k == expected))})
        result["cells"][kind] = {"rendered_sha256": digest(texts), "seconds": a["seconds"], "usage": a["body"].get("usage"), "results": rows}
    return result


def refusal_cell(root, config, kind):
    key = key_from(config.get("key_file"))
    metadata = request(config["url"], "/models", key=key)
    save(root / "models-response.json", metadata)
    if kind == "ambiguous":
        refusal = expect_refusal(lambda: choose_model(metadata), "ambiguous-model-discovery")
        return {"evidence_class": "actual-endpoint-metadata", "public_outcome": refusal,
                "embedding_request_sent": False, "production_api_guarantee": False}
    model, identity = choose_model(metadata, config.get("model"))
    a = request(config["url"], "/embeddings", {"model": model, "input": PROBES[:1]}, key)
    save(root / "nonembedding-response.json", a)
    refused = a["status"] in (400, 404, 405, 422, 501)
    return {"evidence_class": "actual-endpoint-capability", "identity_assurance": identity,
            "public_outcome": {"status": "incompatible" if refused else "unexpected-acceptance", "exit": 1 if refused else 0,
                               "reason": "embedding-capability-refused" if refused else "real-endpoint-accepted-embedding"},
            "response": safe_answer(a), "production_api_guarantee": False}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--ssg", type=Path, required=True)
    p.add_argument("--kind", choices=("embedding", "ambiguous", "nonembedding"), default="embedding")
    args = p.parse_args()
    root = create_run(args.output, "endpoint-" + args.kind)
    bind_sources(args.ssg, root / "source-bindings.json")
    try:
        report = qualify(root, load(args.config)) if args.kind == "embedding" else refusal_cell(root, load(args.config), args.kind)
        save(root / "report.json", report)
        if args.kind == "ambiguous" and not report["public_outcome"]["matched"]:
            raise ValueError("Actual endpoint did not provide the expected ambiguous-model refusal")
        if args.kind == "nonembedding" and report["public_outcome"]["status"] != "incompatible":
            raise ValueError("Actual endpoint did not provide the expected non-embedding capability refusal")
    except Exception as e:
        save(root / "failure.json", {"status": "unavailable", "exit": 1, "error_type": type(e).__name__, "reason": str(e), "production": False})
        raise
    finally:
        save(root / "artifact-inventory.json", inventory(root))


if __name__ == "__main__":
    main()
