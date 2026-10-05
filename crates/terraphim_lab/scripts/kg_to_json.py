#!/usr/bin/env python3
"""Compile Terraphim KG markdown (`synonyms::` lines) into thesaurus JSON.

Used for the two generated files in this crate (run from crates/terraphim_lab):

    # Embedded default style lists (src/lists.rs, DEFAULT_STYLE). Concept names
    # are not added as terms, so "lab-filler" never matches prose.
    python3 scripts/kg_to_json.py style kg kg/lab-style.json

    # Fixture role thesaurus. Concept names are terms too, as the Terraphim
    # thesaurus builder does; ids are assigned in sorted file order from 1.
    python3 scripts/kg_to_json.py role tests/fixtures/domain tests/fixtures/role-thesaurus.json

After changing the role thesaurus or the corpus, regenerate the rolegraph:

    LAB_REGENERATE_FIXTURES=1 cargo test -p terraphim_lab --test rolegraph_conformance

`kg/typos.json` is hand-maintained (misspelling -> correction) and is its own
source. The test `embedded_json_matches_the_kg_markdown` fails if
`kg/lab-style.json` drifts from `kg/lab-*.md`.
"""

import glob
import json
import os
import sys

STYLE_IDS = {"lab-filler": 1, "lab-hedge": 2, "lab-hedge-phrase": 3, "lab-tone": 4}


def synonyms(path):
    text = open(path, encoding="utf-8").read()
    line = next(
        (l.split("::", 1)[1] for l in text.splitlines() if l.strip().lower().startswith("synonyms::")),
        "",
    )
    return [s.strip().lower() for s in line.split(",") if s.strip()]


def main():
    mode, kg_dir, out = sys.argv[1:4]
    data = {}
    if mode == "style":
        name = "Lab style lists (default)"
        for path in sorted(glob.glob(os.path.join(kg_dir, "lab-*.md"))):
            concept = os.path.splitext(os.path.basename(path))[0]
            for term in synonyms(path):
                data.setdefault(term, {"id": STYLE_IDS[concept], "nterm": concept})
    elif mode == "role":
        name = "Lab fixture role"
        for cid, path in enumerate(sorted(glob.glob(os.path.join(kg_dir, "*.md"))), start=1):
            concept = os.path.splitext(os.path.basename(path))[0]
            for term in dict.fromkeys([concept.lower()] + synonyms(path)):
                data.setdefault(term, {"id": cid, "nterm": concept})
    else:
        sys.exit(f"unknown mode {mode!r}: use style or role")
    with open(out, "w", encoding="utf-8") as fh:
        json.dump({"name": name, "data": data}, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(f"{out}: {len(data)} terms")


if __name__ == "__main__":
    main()
