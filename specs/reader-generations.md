# Monotonic reader generations

The shared Database retains a minimum compatible reader generation in the
existing bounded migration status marker. This preserves the old opener's
fail-closed behavior for unknown markers without changing canonical content IDs.

| Marker | Minimum generation |
|---|---|
| absent or `complete` | Original |
| `neuron-v1` | NeuronV1 |
| `auth-v1` | AuthenticatedV1 (includes NeuronV1) |

`copying`, `export-v1` and unknown markers remain unavailable to normal open.
The explicit sealed transfer reader retains its separate source profile.

A trusted host may promote the generation transactionally. Promotion is monotonic:
a request for a lower generation is a read-only success retaining the current
one. Application migration and transfer preparation promote to at least NeuronV1
inside their existing atomic transaction and never overwrite AuthenticatedV1.
The authenticated native HTTP owner interprets AuthenticatedV1 as requiring its
versioned signed submission profile; BBG supplies compatibility fencing, not the
signature verifier or network policy.

Existing executables know neither `auth-v1` nor its requirements and reject open.
A config file ignored by an earlier reader cannot replace this durable guard.
The host quiesces old processes before promotion; raw Database authority remains
a trusted host interface and is never handed to an untrusted program.
