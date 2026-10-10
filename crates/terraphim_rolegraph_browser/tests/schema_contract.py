"""Run with an existing Python jsonschema installation; never installs packages."""
import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator

root = Path(__file__).resolve().parents[3]
schema = json.loads((root / "docs/contracts/schemas/rolegraph-browser.schema.json").read_text())
Draft202012Validator.check_schema(schema)
validator = Draft202012Validator(schema)
steps = json.loads((Path(__file__).parent / "fixtures/scenario.json").read_text())
count = 0
for index, step in enumerate(steps):
    request = {
        "version": 1,
        "requestId": str(index),
        "generation": step["generation"],
        "expectedRevision": step.get("expectedRevision"),
        "op": step["op"],
        "payload": step["payload"],
    }
    if step["expect"]["/ok"]:
        validator.validate(request)
        count += 1

base = {"version": 1, "requestId": "schema", "generation": "0", "expectedRevision": None}
for invalid in [
    {**base, "op": "nodes", "payload": {"limit": 101}},
    {**base, "op": "query", "payload": {"text": "", "mode": "fallback", "skipDirect": True}},
    {**base, "op": "index_learning", "payload": {"document": {"id": "d", "rank": 9007199254740993, "nodes": ["1"]}}},
    {**base, "generation": 0, "op": "stats", "payload": {}},
    {**base, "op": "bogus", "payload": {}},
]:
    assert not validator.is_valid(invalid), invalid

print(f"Schema valid; {count} successful scenario requests and 5 invalid payloads verified")

for op in ["nodes", "edges", "query", "match", "documents_for_term", "learning_documents"]:
    p = {"text": "alpha", "mode": "graph"} if op == "query" else ({} if op in ["nodes", "edges"] else {"text": "alpha"})
    validator.validate({**base, "op": op, "payload": {**p, "offset": 4294967295}})
    assert not validator.is_valid({**base, "op": op, "payload": {**p, "offset": 4294967296}})

numeric_rank = {**base, "revision": "0", "ok": True, "result": [{"id": "1", "rank": 1, "connected_with": []}]}
numeric_rank.pop("expectedRevision")
assert not validator.is_valid(numeric_rank)
if len(sys.argv) > 1:
    responses = 0
    snapshots = 0
    for line in Path(sys.argv[1]).read_text().splitlines():
        if line.startswith("ROLEGRAPH_WIRE "):
            response = json.loads(line.removeprefix("ROLEGRAPH_WIRE "))
            validator.validate(response)
            if response.get("ok") and isinstance(response["result"], dict) and "snapshotText" in response["result"]:
                validator.validate(json.loads(response["result"]["snapshotText"]))
                snapshots += 1
            responses += 1
    assert responses == 33 and snapshots == 1
    print(f"Validated {responses} emitted native responses, {snapshots} actual snapshots and numeric-rank/offset negatives")
