# TreeLedger roadmap

Completed milestone 1: Rust CLI, deterministic relative-path manifests, streamed SHA-256 hashes, explicit scan issues, refusal to compare incomplete results, structural validation, exclusive output creation and synthetic examples/tests.

Remaining candidate work to confirm against genuine personal use:

Completed milestone 2: synced staging files and atomic no-clobber publication, with injected write-failure and destination-race tests.

- Interrupted-scan recovery, orphan-stage cleanup and power-loss durability, including process-crash tests.
- Incremental scans with a clear cache invalidation policy rather than blindly trusting timestamps.
- Bounded-memory traversal and large-directory performance measurement.
- Stronger symlink/path-race handling and cross-platform path semantics.
- Explicit exclusion policies with visible coverage and comparison semantics.
- Packaging and broader compatibility tests.

Do not count these as verified hours or guaranteed programme eligibility. Keep data synthetic. This is a directory-change tool, not a backup or vulnerability scanner.

Completed milestone 3: configurable global traversal-entry budget, explicit incomplete coverage and comparison refusal. Byte-bounded storage and performance benchmarking remain future work.

Completed follow-up: bounded output serialization. See README for scope and remaining limitations.
