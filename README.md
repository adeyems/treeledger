# TreeLedger

A Rust CLI for making and comparing local directory manifests. For developers checking changes in synthetic fixture directories: records relative paths, sizes and streamed SHA-256 content hashes, then reports added, removed and changed files. No Python implementation or runtime is involved.

## Run

Install a current stable Rust toolchain with Cargo, then:

```sh
cargo build --locked
cargo run --locked -- scan examples/before before.json
cargo run --locked -- scan examples/after after.json
cargo run --locked -- compare before.json after.json
cargo test --locked
```

Comparison intentionally exits 1: `demo.txt` changed and `added.txt` was added. The output files must not already exist; choose new filenames for subsequent scans. Comparing a complete manifest against itself exits 0. No containers, accounts or network calls are used at runtime; Cargo downloads dependencies when building.

## Manifest and comparison

Version 1 contains `version`, `complete`, `files` and `issues`. Each file has a slash-separated relative `path`, byte `size` and lowercase `sha256`. Files and issues are sorted for deterministic output on a stable directory. Empty directories are not recorded. Comparisons match exact case-sensitive relative paths, regardless of input order, and check both size and hash. Renames appear as removal plus addition. Timestamps and permissions are not compared.

Scan exit 0 means a complete manifest; 1 means a manifest with explicit issues. Symlinks, special files, unreadable files/directories, non-UTF-8 names, names containing backslashes/colons, and paths deeper than 128 components are skipped with issues. Even an intentionally skipped symlink makes the scan incomplete. Compare refuses incomplete manifests (exit 2), never silently claiming they match. Invalid input and I/O failures also exit 2 with JSON errors on stderr. Successful reports go to stdout; comparison exit 1 means differences.

The output must be outside the scan root, in an existing parent directory, and is created exclusively so an existing file is not overwritten. A write failure can leave a partial new output file; crash-safe atomic publication is future work. Input/output manifests are limited to 16 MiB; the scan currently collects all metadata in memory before enforcing its output limit. Contents are hashed in 64 KiB chunks, not loaded whole.

## Boundaries

Use trusted, quiescent synthetic directories only. This is not a filesystem snapshot, backup, sandbox or adversarial security tool. Symlinks observed during traversal are skipped, but concurrent path replacement creates check/open races. Length/modification-time checks detect some file changes during hashing, not every race or same-size rewrite. Do not use against directories other processes are modifying. No consistency guarantee spans multiple files.

Manifests disclose filenames, sizes and hashes; hashes are not anonymization. Do not scan customer/personal data, credentials or confidential directories for this project. Root symlinks are rejected. Windows is not currently covered by CI; Linux and macOS are the initial targets. Hard links are represented as separate paths. No incremental scans, recovery, exclusions, parallel hashing or signed manifests yet.

MIT licensed. See `ROADMAP.md`; no claim of production readiness or programme eligibility.
