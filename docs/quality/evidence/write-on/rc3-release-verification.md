# rc.3 release asset verification

Release `terraphim_lsp-v1.22.0-rc.3` (prerelease) in the public, release-only repository `terraphim/terraphim-lsp-releases` on the tailnet Gitea. Both files are downloaded with no credentials. The binary is the one the 32-check protocol run used (`protocol-rc3-released.json`); `cmp` below confirms they are identical. The macOS binaries are unsigned.

```
2026-10-08 14:56 BST
$ curl -sSfLO (no credentials) https://git.terraphim.cloud/terraphim/terraphim-lsp-releases/releases/download/terraphim_lsp-v1.22.0-rc.3/checksums.txt
$ curl -sSfLO (no credentials) https://git.terraphim.cloud/terraphim/terraphim-lsp-releases/releases/download/terraphim_lsp-v1.22.0-rc.3/terraphim-lsp-universal-apple-darwin
$ cat checksums.txt
699fe7d7b842aba94ebbd6102613e5bace54c3b7a439a490258bf5cecfbb7cf7  terraphim-lsp-aarch64-apple-darwin
281a9d904bf7d0ae910b87afffd1e5fcf11e66e0e360df0fe5c66d5ca7fd71fb  terraphim-lsp-aarch64-unknown-linux-musl
e3488ea18e9c9502c243c57fd6feb90e4f06b8e4d63767e349def479ae4b652d  terraphim-lsp-universal-apple-darwin
176f28e8402af3d6cc3ae7f0135b41b1c0ef49fc2bb7e12ed6f38a2cf85ab7f1  terraphim-lsp-x86_64-apple-darwin
eebfdb00987711297b2ce9ceef977676ce39a83eabbe9912237f96d7b7658cbc  terraphim-lsp-x86_64-unknown-linux-gnu
068e7313c74a8de25130c7297bd86c815e01fa165e6a4c0b904dbcca5ce71b60  terraphim-lsp-x86_64-unknown-linux-musl
$ shasum -a 256 -c --ignore-missing checksums.txt
terraphim-lsp-universal-apple-darwin: OK
exit 0
$ shasum -a 256 terraphim-lsp-universal-apple-darwin
e3488ea18e9c9502c243c57fd6feb90e4f06b8e4d63767e349def479ae4b652d  terraphim-lsp-universal-apple-darwin
$ cmp with the binary the protocol run used (<work>/verify/rc3-released/)
identical
$ file
terraphim-lsp-universal-apple-darwin: [x86_64:Mach-O 64-bit executable x86_64] [arm64]
terraphim-lsp-universal-apple-darwin (for architecture x86_64):	Mach-O 64-bit executable x86_64
terraphim-lsp-universal-apple-darwin (for architecture arm64):	Mach-O 64-bit executable arm64
```
