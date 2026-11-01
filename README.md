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

The output must be outside the scan root, in an existing parent directory, and is created exclusively so an existing file is not overwritten. Output is written to a unique temporary file beside the destination, synced, then published atomically without replacing an existing path. Ordinary write/publication failures clean up the temporary file. A process kill may leave a `.treeledger-` temporary file; it is not a published manifest. Directory-entry durability across power loss is not guaranteed. Interrupted scans still restart from scratch. Input/output manifests are limited to 16 MiB; the scan collects metadata under its entry-count budget, then streams serialization through the output limit. Contents are hashed in 64 KiB chunks, not loaded whole.

## Boundaries

Use trusted, quiescent synthetic directories only. This is not a filesystem snapshot, backup, sandbox or adversarial security tool. Symlinks observed during traversal are skipped, but concurrent path replacement creates check/open races. Length/modification-time checks detect some file changes during hashing, not every race or same-size rewrite. Do not use against directories other processes are modifying. No consistency guarantee spans multiple files.

Manifests disclose filenames, sizes and hashes; hashes are not anonymization. Do not scan customer/personal data, credentials or confidential directories for this project. Root symlinks are rejected. CI now includes Windows alongside Linux and macOS. Unix-only permission and symlink tests are gated; path matching remains case-sensitive in manifests even on case-insensitive filesystems. Windows support is experimental and does not imply cross-platform filename equivalence. Hard links are represented as separate paths. No incremental scans, recovery, parallel hashing or signed manifests yet. Explicit exclusions are described below.

MIT licensed. See `ROADMAP.md`; no claim of production readiness or programme eligibility.

## Traversal budget

Use `scan ROOT OUTPUT --max-entries N` to limit visited entries (1–100000; default 100000). Files, directories, skipped paths and directory-entry errors consume the shared budget. If more entries remain, scanning stops globally with an `entry_limit` issue and exit 1; comparison refuses the incomplete manifest. The retained subset depends on filesystem traversal order. This caps entry count, not byte memory, file sizes or elapsed time; the 16 MiB serialized-output limit still applies.

## Bounded output serialization

Manifest JSON now streams into the temporary staging file through a 16 MiB writer budget, rather than allocating a complete serialized buffer. Oversized serialization removes staging and publishes no final manifest (exit 2). An exact-limit manifest is allowed. Entry metadata is still collected in memory subject to the traversal-entry budget; this does not provide a strict total-memory bound.

## Explicit exclusion policies

Repeat `--exclude cache` or `--exclude fixtures/generated.json` to omit an exact relative path and its descendants. Paths are case-sensitive with slash separators, not globs; traversal and absolute paths are rejected. Rules are sorted and deduplicated, limited to 100 paths of 4096 bytes each. Scoped manifests use version 2 and include exclusions; complete means complete only within this declared scope. Excluded entries consume the traversal budget but their descendants are not visited. The report does not enumerate every omitted descendant. Comparisons reject different policies and manifests containing excluded entries. Version-1 manifests without exclusions remain readable; scans without rules still emit version 1.
