# Sealed application transfer

An application transfer copies existing content, heads, history, request receipts
and uniqueness claims into an existing shared Database without changing their
bytes or the backend. It introduces no protocol identity. The source must be an
application-only SSD store; redb conversion is a separate tool.

The source is opened under the backend's exclusive process lock. Inspection pins
all namespaces/heads (at most 256), the target subject and a caller transfer key.
A seal is committed before copying. Its status rejects ordinary Database open in
both old and current binaries; only the explicit transfer reader can reopen it.
The application writer checks the seal even through an existing Database clone.
The source remains retained for reads and forward recovery; ordinary legacy
execution is never silently re-enabled by a failed or interrupted transfer.

Target preparation atomically reserves every source namespace and sets the
generation marker that rejects old binaries. Existing unrelated namespaces,
native graph state and content remain usable. A colliding namespace rejects the
transfer. A staged namespace cannot accept application writes or participate in
semantic activation until transfer completion. No neuron root is created here.

Copying uses pages bounded by transaction byte/key limits and an atomically
advanced cursor. Exact duplicate content/claims are accepted; different bytes
under the same key fail. Missing pages cannot be skipped. All source records,
content identities, references, history continuity and receipt coverage must
validate before the completion marker is committed. The caller's graph codec
validates content; BBG does not implement a second content hash/record codec.

The transfer manifest binds the target, transfer key, pinned source heads and
source transaction marker. Reruns require that exact manifest. After completion,
the authenticated neuron importer performs its own schema/resource/authority
validation and atomic mapping activation. A transfer alone grants no rights to
execute the old work. Unknown attempts and every old receipt remain unchanged.

Cancellation retains the sealed source and inert target staging as evidence.
Deletion or restoration of a previous writer requires a separate verified
retention/recovery operation. A failed transfer is resumed forward with the same
manifest, not by copying open database files or launching the old dispatcher.


## Read-only source inspection and logical export

An explicit archive reader can open an existing application-only SSD source,
including a sealed source, without the target key or execution authority. It
holds the source process lock, exposes only reads, and makes no application or
seal transaction. Backend opening/recovery may perform its normal filesystem
maintenance; logical history and its last transaction marker remain unchanged.
An absent/empty path is an error rather than creation of an empty history.

Inspection reports exact namespace heads, existing seal/target/transfer nonce,
row counts, logical key/value bytes and a deterministic archive digest. Bounded
validation checks every row, receipt/history coverage, selected heads and content
closure through the graph codec. The default inspector accepts at most one
million rows and eight GiB of logical bytes; smaller caller bounds are allowed.
Exceeding a bound returns an explicit incomplete/error result, never a pass.
The neuron reader additionally validates legacy schemas, artifacts, cumulative
resources and continuation compatibility without dispatch.

Logical export uses the same sealed transfer protocol into a BBG directory.
It requires an explicit intended target subject and stable transfer nonce, but
needs no signing key or live target root: these fields bind future activation,
not current permission. It seals the source, reserves inert target namespaces
and copies validated pages. Reopening with the same paths/target/nonce resumes.
The exported directory retains all application content, history, requests and
claims in their existing bytes; it is a staged archive, not an active old writer.
It can be inspected with explicit legacy readers and activated later only through
the authenticated semantic importer. No additional archive codec is introduced.

The digest is hemera H over `bbg/application-archive-rows/1\0`, followed by
five tables in this exact order: Content, Claims, Requests, History, Heads.
Each table contributes its zero-based ordinal as one byte, even when empty.
Each lexicographically ordered row contributes key length as little-endian u64,
key bytes, value length as little-endian u64 and value bytes. Seal and migration
metadata are excluded; sealing preserves the logical archive digest. Counts and
sizing include logical keys and values, independent of backend compression.

Inspectors distinguish exact logical byte counts from filesystem capacity.
Destination available bytes are an observation of the selected filesystem.
A conservative planning estimate includes extra backend/log/compaction headroom;
it is not a reservation or a promise that space cannot run out. Every persistence
failure still propagates, leaves retained source data and permits forward resume.
