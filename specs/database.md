---
title: shared storage transactions
tags: bbg, storage, spec
---
# shared storage transactions

Database owns a physical BBG store. The SSD profile uses Fjall; the HDD profile
uses redb. Callers choose the profile and path once. ApplicationStore and disk
ShardStore views share an owner, exclusive filesystem lock and failure state.
The working profile is SSD; HDD is available for explicitly selected storage
and archive use. Attaching a COLD store does not create a cross-device transaction.

## transactions

`transaction(closure)` serializes conditional reads, bounded staging and one
durable backend commit. Reads see earlier writes in the same closure. A closure
error discards every staged operation. One successful commit atomically publishes
all affected shard and application tables. Clones share the serialization lock.
Closures access the supplied Transaction rather than reentering Database or
another view of that owner. Reentry would deadlock and is prohibited.

Typed shard access validates dimensions, canonical field encoding and the
existing shard value bound. EPHEMERAL is excluded from disk transactions.
Application values remain opaque bytes with 32-byte content keys, 40-byte
history keys and 64-byte receipt keys. Raw table access is crate-private.
The native coordinator accesses a closed `RecordDomain` set for exact native
state, history, receipts, metadata, balances, blocks and compatibility export.
These bounded byte records share the same transaction and failure latch as
polynomial shards and applications. They preserve complete integer encodings;
the coordinator supplies and validates each record's versioned schema.
Transactions coalesce updates by table/key, with at most 150000 keys and
32 MiB of combined key/value bytes. Staging reserves two keys and 91 bytes for
the automatic recovery markers within those limits. Each raw key is at most 64 bytes; one value
or read is at most 16 MiB + 1 byte. Scans have explicit entry and byte budgets,
at most 4096 entries and 32 MiB per page. A record that cannot fit an empty
page returns a limit error. Scans require no staged writes in that table.

## commit outcomes

A Commit contains the closure's value and an optional change identity; a
read-only transaction has no new identity or durability acknowledgement.
The `bbg/database-batch/v1` identity binds the sorted table names, keys,
operation kinds, lengths and final values. It is a change identity rather than
an authenticated graph root or an application request identity.

`last_transaction` is stored atomically with every nonempty transaction.
A transaction modifying shards also updates the legacy `last_commit` shard
marker to that transaction's identity. Application-only commits preserve the
last shard marker. Existing stores without these markers remain readable.
Application request receipts retain the complete retry history independently
of the latest transaction marker.

Fjall uses the repaired batch path with SyncAll. Redb uses one Immediate write
transaction. Errors before the backend commit are explicit failures; commit
errors return CommitUnknown with the change identity and freeze every shared
view. No later transaction can overwrite the recovery marker while the outcome
is unresolved. Drop all views, reopen and resolve before dependent publication.
The OS releases the exclusive lock only after the final owner is dropped.

Disk ShardStore views retain their private staged batches on failure and clear
them only after success. Shared committed changes are visible through owned
disk reads. A HOT cache attached elsewhere must be invalidated by its owner
when publishing changes through another view; shared database ownership alone
does not synchronize arbitrary external caches.

Existing shard tables and value encodings are retained. New working directories
are private (0700 on Unix), new redb files are 0600, and parent-directory
durability is synchronized on open. Wrong path kinds and incomplete migrations
are errors. Physical power-loss and archive-copy qualification remain separate
acceptance work in the storage reliability roadmap.
