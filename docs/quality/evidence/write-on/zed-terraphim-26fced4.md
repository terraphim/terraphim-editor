# zed-terraphim at `26fced4`

Main after #32 (pins rc.3, provisions from the internal release repository). Re-run on 2026-10-08 with the steps of `.gitea/workflows/ci.yml`.

Gitea CI on this commit: success ci / build (push) success 2026-10-08T12:14:33+02:00.

```
26fced425584fca66fc01499fa7fd6151004953c
fmt exit 0
clippy native exit 0
clippy wasm32-wasip2 exit 0
test exit 0
wasm build exit 0
wasm: see guards.sh I4 below
guards exit 0
package-tests exit 0

cargo test:
test result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

scripts/guards.sh:
I4 ok: no terraphim_* crates in the dependency tree
I4 ok: wasm is 357877 bytes (budget 450000)
I5 ok: extension.toml declares no capabilities

scripts/package-tests.sh:
----------------------------------------------------------------------
Ran 20 tests in 0.032s

OK
```
