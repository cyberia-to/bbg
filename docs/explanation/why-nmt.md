---
tags: cyber
crystal-type: entity
crystal-domain: cyber
---
# why NMTs cannot be replaced

sorted polynomial commitments (as proposed in bbg v0.9) can approximate completeness but lack structural guarantees.

**polynomial completeness requires a protocol.** prove boundary entries, prove contiguity, prove sorting was maintained. each step requires a separate argument. any step can have bugs.

**NMT completeness is a structural invariant.** the tree physically cannot represent a valid root over misordered leaves. there is no protocol to debug because there is no protocol — just a tree.

**DAS requires NMTs.** namespace-aware Data Availability Sampling needs namespace labels propagated through internal nodes. polynomials don't have internal nodes.

**sync requires NMTs.** "give me everything for neuron N with proof nothing is hidden" is the cybergraph's fundamental operation. NMT completeness proofs make this trustless. polynomial approaches require trusting that sorting was maintained by consensus.

**production evidence.** Celestia has processed millions of blocks with NMT-based DAS since October 2023. no production system uses sorted polynomial completeness proofs.

NMTs provide completeness (all items in namespace). WHIR polynomial commitments provide efficient membership and batch proofs within individual leaves. NMTs structure the tree; WHIR proves evaluation claims against leaf data.

see [[indexes]] for NMT structure, [[data-availability]] for DAS, [[WHIR]] for evaluation proofs
