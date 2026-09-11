---
title: atomic application storage
tags: bbg, storage, spec
status: implementation
---
# atomic application storage

ApplicationStore stores cybergraph application content, ordered history, request
deduplication and conditional heads in one redb transaction. It is enabled by
backend-hdd. The data belongs to the local graph session and carries local
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
Immediate durability.
Failure before commit leaves the old selected head. A commit error is Unknown;
callers stop dependent dispatch and reopen/resolve the request before retrying.

Reads return structured errors and enforce caller bounds. History uses namespace
and big-endian index keys for bounded range reads. A database is exclusively
opened by redb; multiple writer handles cannot independently advance a head.
The owning directory must already exist and is synced at open. On Unix a newly
created file has mode 0600. Existing stores retain their permissions.

This interface is independent of the polynomial ShardStore cache interface.
Applications use this transactional path for history; shard roots retain their
own existing meaning. The storage implementation performs no cell evaluation.
