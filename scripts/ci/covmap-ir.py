#!/usr/bin/env python3
"""Keep only the coverage mapping of an LLVM IR file (issue #100).

usage: covmap-ir.py IN.ll OUT.ll

rustc's `-Cinstrument-coverage --emit=llvm-ir` output for wasm32 carries the
coverage mapping in three kinds of global: the `__llvm_coverage_mapping`
header (section `__llvm_covmap`), one `__covrec_*` record per function
(`__llvm_covfun`) and the function names (`__llvm_prf_nm`,
`__llvm_prf_names`). llvm-cov reads them from an object file, but the
WebAssembly object writer puts `__llvm_prf_names` in a data segment rather
than a custom section, so llvm-cov finds "no coverage data" in a wasm object.

These globals are plain constants (no pointers), and their section names are
the ELF ones, so this script writes a module holding only them plus an
`llvm.used` list that keeps them, for clang to compile as an x86_64 ELF
object. llvm-cov then reads the mapping from that object; the counters come
from the profraw files the wasm tests wrote.
"""

import re
import sys

KEEP = re.compile(r"^@(__covrec_\w+|__llvm_coverage_mapping|__llvm_prf_nm) = ")


def main() -> None:
    src, dst = sys.argv[1], sys.argv[2]
    names = []
    out = []
    with open(src, encoding="utf-8") as f:
        for line in f:
            m = KEEP.match(line)
            if not m:
                continue
            # Comdats and visibility do not matter for a data-only object.
            line = re.sub(r", comdat(\(\$[\w.]+\))?", "", line)
            out.append(line)
            names.append(m.group(1))
    if "__llvm_coverage_mapping" not in names:
        sys.exit(f"{src}: no coverage mapping (was it built with -Cinstrument-coverage?)")
    used = ", ".join(f"ptr @{n}" for n in names)
    with open(dst, "w", encoding="utf-8") as f:
        f.write("".join(out))
        f.write(f'@llvm.used = appending global [{len(names)} x ptr] [{used}], section "llvm.metadata"\n')


if __name__ == "__main__":
    main()
