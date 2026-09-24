---
title: durable content storage
tags: bbg, storage, file, spec
status: accepted
---
# durable content storage

This specializes [[soft3/specs/storage|the stack storage contract]] for BBG.
It defines the target content service under the [shared Database owner](database.md).
Implementation and qualification follow [[soft3/roadmap/storage/README|the storage project]].
Operation names below describe contracts; concrete Rust signatures and physical
key layouts are fixed in that project's interface work package.

## ownership and records

The selected Database backend owns payload parts and their descriptors, indexes,
verification evidence, transfer progress, retention roots and recovery receipts.
Fjall and redb expose the same logical contract. Small-value inlining is an
internal layout optimization under the same owner. Large content is divided
across bounded values; file size is independent of an individual value limit.

| record | binds | lifetime |
|---|---|---|
| Content descriptor | Canonical particle, interpreted length, representation and required closure | Immutable after validation |
| Part | Bytes and position or structural membership under the descriptor | Until every retention/staging/read obligation is released |
| Coverage index | Precisely which ranges are present and verified, with verifier/profile binding | Updated transactionally with the corresponding parts |
| Staging record | Namespace, operation identity, expected content, progress and reservation | Resume, publish, cancel or explicitly expire |
| Retention root | Authorized obligation over a descriptor/closure, with its release rule | Until explicit release or agreed expiry |
| Publication receipt | Request identity, selected head and committed outcome | Application retry/recovery policy |

FS namespace bindings, channel heads and patch records are application graph
data under this same owner. FS/Cybergraph define name resolution and patch
semantics; BBG stores their records and paged indexes. Retaining a filesystem
revision protects its namespace/patch state and required content closure.
Removing one alias cannot release content still retained by another obligation.
Private paths and directory membership remain in their authorized scope.

Descriptors, closure indexes and progress are themselves paged where needed.
No single manifest or in-memory collection must grow to contain all files,
parts or revisions. Internal part keys remain distinct from public file identity.
Sharing identical parts is allowed only within a qualified privacy scope.

## capabilities

### local streamed-content API

`ContentStore::from_database` attaches to the existing owner. `begin(upload,
spec)` binds `Upload { namespace, request }` to expected particle, length,
verifier profile and physical part size. Retry must preserve that binding.
`write_part` accepts exact-sized parts in any order, with a shorter final part;
identical retries succeed and conflicting bytes fail. Payload and checksum/
coverage records commit together. Part size is a per-operation budget, independent
of the total file size. `uploads` and `coverage` page through durable state.

`verify(upload, verifier)` creates a bounded verification session over immutable
parts. The trusted host supplies the verifier implementation matching the bound
profile. Each `step` reads a bounded number of parts, checks stored checksums and
feeds the canonical verifier. Completion checks particle and length before
publishing the immutable namespace-scoped descriptor. Interrupted verification
restarts hashing from the beginning; already stored parts survive. This interface
supplies full-stream verification, with range-proof qualification remaining S1.

`read_range` reads bounded ranges of sealed content and validates stored part
checksums. It provides local integrity, not a transferable range proof. A shared
`Transaction::retain_content` binds a sealed descriptor to an application root
inside the application's conditional publication transaction. Namespace scoping
is storage isolation; the host/Cybergraph supplies authorization.

`cancel` marks a noncanonical upload cancelled and reclaims its parts in bounded
batches. A cancelled request stays bound and cannot be reused. Canonical sealed
content remains protected; releasing it and collecting its retention roots is
the separate S4 work package. A repeated upload of already sealed content can
resolve to the existing descriptor and cancel its redundant staged parts.

Cybergraph's initial verifier preserves existing `Content::Blob` Hemera identity.
Its explicit profile is a compatibility boundary pending S1, not a final choice
of the stack's future structured-file construction.

| capability | required semantics |
|---|---|
| Begin / resume write | Bind namespace, operation identity, verifier and resource reservation; detect incompatible retry |
| Write part | Bounded, idempotent durable append; reject conflicting bytes; record validated coverage atomically |
| Query missing ranges | Paged local coverage with a stable cursor or explicit restart condition |
| Seal | Incrementally validate canonical identity, total length and complete required closure; return a protected readiness handle |
| Read / read range | Stream under an authorized lease; distinguish absent, partial, unverified and corrupt content |
| Publish / retain / release | Conditional operations within the owner's transaction and failure latch |
| Cancel / reclaim | Resumable release of staging and unreachable content subject to all live obligations |

BBG consumes the selected canonical verifier. Untrusted bytes remain quarantined
until verification; received proof metadata alone cannot set verified coverage.
Sparse delivery is accepted only for ranges authenticated against the requested
particle. Full-file verification may require a complete stream under the chosen
identity profile. Local disk reads enforce the profile's integrity checks;
detected corruption invalidates availability and reports repair requirements.

No capability performs network I/O. A local miss returns structured coverage
to Cybergraph/Foculus. Radio receives a narrow source/sink adapter from the host
and never opens a second store. Durable transfer jobs use BBG records through
their owning service.

## bounded preparation, atomic publication

Existing [transaction and page budgets](database.md) bound work per call. A file
or history can span any number of such calls. Preparation writes immutable
parts in durable batches protected by a staging root. Seal walks the required
closure incrementally and binds its readiness handle to the Database owner,
namespace, descriptor and protected generation.

Publication checks that handle, the expected prior application head and request
identity in one bounded transaction. It records the new head, its retention
root and retry receipt together, and transfers the staging obligation. The
handle MUST remain protected against reclamation throughout validation and
publication. Its implementation must prevent forged or stale readiness claims.

Large closures use a protected immutable root plus an incremental validation
record. Publication cost is bounded by metadata; it does not atomically insert
every payload part or update a reference counter for every reachable object.
The [application storage](application-storage.md) view shares this transaction.

Concurrent publications use conditional head updates. A losing request leaves
its staged content eligible for an explicit retry or cancellation, while the
winning revision stays retained. Publishing a reference to remotely held data
must preserve its explicit absent/partial local status.

## failure and recovery

Durable acknowledgement follows Fjall SyncAll or redb Immediate through the
Database contract. Errors before commit preserve the previous published state.
CommitUnknown freezes all views; reopen and resolve the recorded outcome before
continuing. Transport acknowledgements are separate from durability receipts.

Recovery enumerates unfinished staging and maintenance work in bounded pages.
It can resume from verified persisted coverage or discard explicitly cancelled
work. It MUST preserve previously acknowledged heads and their complete local
closure, or report detected corruption and the need for repair. A restart must
never promote an unfinished file based only on an availability flag.

Migration between devices uses copy, verify, durable destination receipt,
conditional route update, then source release when policy allows. Each device
has its own Database owner; interrupted migration preserves a recoverable copy.
The source remains authoritative until the destination obligation is established.

## retention and reclamation

The live set includes publication roots, required history, recovery checkpoints,
replica obligations, staging and active reads. Retention ownership is explicit;
a content identifier or graph reference alone supplies no deletion authority.
Release and expiry are separate from immutable file identity.

Reclamation uses bounded, resumable work and a defined concurrency barrier.
The implementation must specify how a new root or read lease protects content
during an ongoing sweep and how recovery resumes that sweep safely. Reference
counts alone require a complete crash-safe accounting proof before they can
authorize deletion. Active streams must not lose parts midway through a read.

Available-space pressure rejects or throttles new work explicitly. It never
silently removes retained content. A cancelled upload may release reservations
incrementally. Configuration quotas govern capacity admission; they do not
change the address space or truncate historical records.

## privacy and projections

Local application bytes and retention indexes are private to their authorization
scope. Public BBG state is an explicitly selected projection. Storing a ciphertext,
part hash, path, receipt or availability record MUST NOT automatically add it to
public indexes. Deduplication and diagnostics respect the same separation.

The files dimension may describe availability evidence. Physical presence,
integrity, complete closure and future retention remain separate facts. A graph
proof authenticates its statement; recovery also requires the actual content
and a valid freshness decision from the application or sync profile.

## conformance

Both backend profiles must pass [[soft3/roadmap/storage/acceptance|the shared matrix]],
including crash cuts at every lifecycle boundary, concurrent retention/GC,
private namespaces, range verification, large histories and consumer recovery.
Results, commands and revisions belong in `audit/`; executable gates accompany
the implementing work packages. This contract claims no already-passing gate.
