---
title: Committed node storage dependency
date: 2026-09-23
status: tested dependency candidate
---

> Recorded from cyberia-to/bbg#22 (branch `fix/node-storage-dependency`), closed 2026-09-24 as superseded by #9; it describes that narrower candidate, whose code is an earlier variant of this branch. Evidence preserved as written.
# Committed node storage dependency

The existing committed Cybergraph application adapter requires BBG 0.3 APIs that
were still uncommitted: coordination, resumable namespace transfer/archive,
reader generations and authenticated public query context. This candidate
publishes the coherent storage/query contract with Lens 0.2 and Zheng 0.4.

Transfers reserve namespaces, advance data and cursor transactionally, validate
coverage before activation, and bind the activation target. Reader generation
promotion rejects incomplete and unknown markers. Database clones share
coordination locks. Raw database access remains a trusted authority.

Public certificates bind exact complete tables to a caller-pinned root; query
verification also binds namespace, cell index or entity key. Private balance
dimensions are not implicitly disclosed. The legacy recursive opening path
fails closed. Experimental Zheng state-execution proving is excluded; certificates
authenticate reads but do not by themselves prove program execution.

Validation: `cargo test --manifest-path rs/Cargo.toml --all-features --locked`
passed 165 tests on macOS arm64 with zero ignored tests. A bounded independent
review found no concrete new authentication or activation bypass. This is not a
cryptographic security review or power-loss endurance test.

Upgrade boundary: dimension encoding version 2 changes roots and cell offsets;
large dimensions use the documented Hemera commitment branch. Native metadata
still uses version 1. Existing older committed databases have not been shown
to migrate transparently and may fail exact replay/root comparison. Preserve
such stores and establish migration separately; do not overwrite or implicitly
reinitialize them. The fresh node/restart tests validate this candidate's own
format, not upgrade compatibility from earlier committed roots.
