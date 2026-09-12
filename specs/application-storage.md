---
title: atomic application storage
tags: bbg, storage, spec
status: implementation
---
# atomic application storage

ApplicationStore stores cybergraph application content, ordered history, request
deduplication and conditional heads through BBG's shared Database transaction
owner. `open(directory)` selects the SSD/Fjall working profile; `from_database`
attaches to an already opened owner without opening another database. Backend
selection belongs to the owner. ApplicationStore contains no engine-specific
transaction implementation. The data belongs to the local graph session and carries local
durability; it contributes no claim about network consensus or BBG polynomial
commitments. Cybergraph owns content/schema/authentication validation.

An application namespace and all content/request identifiers are 32-byte
particles. Head is (index, commit). Birth uses index zero with no predecessor.
Each successor requires the exact preceding head and index + 1. Integer overflow
and changed content at an existing request/content identity are conflicts.

Each write binds a request fingerprint to its resulting head. An identical retry
returns the original head even after later commits. A changed fingerprint is
rejected. A write may also assign up to 4096 store-wide immutable key/value
claims. A changed value conflicts across application namespaces; a rejected
write retains no claim. Content, history, head, claims and dedup updates share
the selected owner's disk durability barrier.
Failure before commit leaves the old selected head. A commit error is Unknown;
all views sharing the owner stop dependent dispatch and reopen/resolve the
request before retrying. Successful retries retain their original result even
after subsequent application commits.

`apply_with(write, closure)` checks the request receipt before invoking the
closure. For a new request the closure may read, put and delete typed shards
through the same transaction. Application records, shard changes, request
receipt and recovery markers commit together. Rejection rolls back all of
them. Identical retries never invoke the closure again. The caller's request
fingerprint must bind all inputs controlling the shard transition. Unrelated
pending writes in a ShardStore view are not implicitly included.

Reads return structured errors and enforce caller bounds. History uses namespace
and big-endian index keys for bounded range reads. A database is exclusively
opened by its backend; cloned views share one writer lock held from conditional
reads through commit. Multiple handles cannot independently advance a head.
The parent directory must already exist and is synced at open. On Unix a newly
created Fjall directory has mode 0700 and a new redb file has mode 0600.
Existing stores retain their permissions.

This interface and the polynomial ShardStore cache interface are typed views
over the same Database owner. Shard roots retain their own existing meaning.
The storage implementation performs no cell evaluation.

## legacy application migration

Opening an existing redb file as the default working store returns an explicit
format/migration error. It never creates a replacement empty session.
`migrate_redb(source, destination)` requires both backend features and imports
the five legacy application tables into a fresh Fjall directory. Source data
is retained. The source is locked for the export. Copying uses bounded pages
and destination batches. An import-in-progress marker is durable before data
copying; ordinary opens reject an incomplete import, including aliases of the
destination path. Destination creation is exclusive. Completion is published
only after validating record encodings, copying all records, verifying
source/destination contents and checking contiguous history with exactly one
receipt per accepted entry. A temporary disk index bounds receipt validation
memory and is removed before completion. A failed destination is retained for diagnosis;
retry uses a fresh destination. Existing destinations are never overwritten.
