+++
title = "Availability contracts"
description = "The normative meaning of the none, dc, and ha mutation modes: what each acknowledgement promises, what a partition refuses, and how much is at risk on failure."
weight = 7
+++

This page is the contract later availability work implements, not a description of code that exists today. It fixes the
observable meaning of three mutation modes, `none`, `dc`, and `ha`, so an implementation can be judged against a promise
made before it was written. Everything peryx ships today is the `none` contract: one writer with local durability and
operator-driven [failover](@/core/high-availability.md). `dc` and `ha` name the stronger promises that mode selection
will later offer.

The unit of a promise is one **mutation**: a client request that changes authoritative state. peryx has six, three per
ecosystem, and the contract covers each by name. Cache fills change local state too, but they are **reconstructible**,
recoverable from the upstream that served them, so they carry a weaker promise and never gate on a peer.

## Two durability subjects

A mutation touches up to two subjects that fail and recover differently, so the contract tracks them apart:

- **Metadata**: the small authoritative record, an upload entry, a yank flag, a file-to-digest mapping, a manifest, a
  tag, and the serial that orders it. Losing it loses the decision itself.
- **Artifact bytes**: the [content-addressed](@/core/glossary.md#artifact) wheel, sdist, or blob a metadata record
  points at. A digest names bytes that either match or do not; a reader that has the metadata but not the bytes knows
  exactly what it is missing.

The split is why `ha` can acknowledge a mutation before its bytes travel: the digest is durable in metadata, and the
bytes converge behind it. A reader never sees a metadata record resolve to the wrong bytes, only to bytes not yet
present, which it reports rather than guesses.

{% mermaid() %}
flowchart LR
none["none"] --> nl["local durability"]
dc["dc"] --> dd["same-DC durability"]
ha["ha"] --> hm["remote metadata durability"]
ha --> hb["background byte convergence"]
class none,dc,ha accent
class nl,dd,hm good
class hb warn
{% end %}

## What each mode acknowledges

An **acknowledgement** is the success peryx returns to the client. Its meaning is the whole contract: what must be
durable before that response is sent.

- **`none`** acknowledges once the mutation is durable on this node's local storage: the metadata record is committed
  and the bytes are `fsync`'d and renamed into the store. No copy exists elsewhere. This is a single failure domain by
  design.
- **`dc`** acknowledges once the metadata is durable in a second failure domain within one datacenter, and the bytes are
  durable there too. Both subjects are synchronous, paid for by a low-latency intra-DC link. It survives the loss of one
  node's storage; it does not survive the loss of the datacenter.
- **`ha`** acknowledges once the metadata is durable in a remote failure domain, another datacenter. The bytes are
  **not** required to be remote at acknowledgement; they converge in the background. A metadata mutation survives the
  loss of the writing datacenter; a byte that had not yet converged does not, and a reader elsewhere sees a resolvable
  digest whose bytes are still in flight.

These are the only three promises. An implementation that acknowledges before its mode's evidence is complete has broken
the contract regardless of how fast it is.

## The mutation matrix

Each table below is one mutation. The columns are the three modes; each cell names the durability evidence that must be
complete before acknowledgement and the client-visible result. The **partition** and **retry** rows state what happens
when a required failure domain is unreachable and whether a client may safely repeat the call.

### upload: publish a wheel or sdist to a hosted index

| Aspect              | `none`                  | `dc`                                   | `ha`                                              |
| ------------------- | ----------------------- | -------------------------------------- | ------------------------------------------------- |
| Durability evidence | metadata + bytes local  | metadata + bytes durable in-DC         | metadata durable remote; bytes converge after ack |
| Client result       | file resolvable locally | file resolvable after failover         | digest resolvable; remote bytes may lag           |
| Partition response  | commit locally          | refuse with `503` until peer reachable | refuse with `503` until remote metadata reachable |
| Retry               | idempotent by digest    | idempotent by digest                   | idempotent by digest                              |

### yank: mark a file yanked (PEP 592, reversible)

| Aspect              | `none`                    | `dc`                                   | `ha`                                     |
| ------------------- | ------------------------- | -------------------------------------- | ---------------------------------------- |
| Durability evidence | metadata local (no bytes) | metadata durable in-DC                 | metadata durable remote                  |
| Client result       | flag applied              | flag survives failover                 | flag survives datacenter loss            |
| Partition response  | commit locally            | refuse with `503` until peer reachable | refuse with `503` until remote reachable |
| Retry               | idempotent (set to state) | idempotent (set to state)              | idempotent (set to state)                |

### delete: remove a file or project from a hosted index

| Aspect              | `none`                           | `dc`                                   | `ha`                                     |
| ------------------- | -------------------------------- | -------------------------------------- | ---------------------------------------- |
| Durability evidence | metadata removed local           | removal durable in-DC                  | removal durable remote                   |
| Client result       | name gone locally                | removal survives failover              | removal survives datacenter loss         |
| Partition response  | commit locally                   | refuse with `503` until peer reachable | refuse with `503` until remote reachable |
| Retry               | idempotent (absent stays absent) | idempotent (absent stays absent)       | idempotent (absent stays absent)         |

Byte reclamation is separate from the removal: a shared digest is collected only when no metadata references it, so a
delete's client result never waits on byte deletion and never races a concurrent upload of the same digest.

### cache fill: store an artifact fetched from an upstream

| Aspect              | `none`                             | `dc`                              | `ha`                                |
| ------------------- | ---------------------------------- | --------------------------------- | ----------------------------------- |
| Durability evidence | metadata + bytes local             | metadata in-DC; bytes converge    | metadata remote; bytes converge     |
| Client result       | served while caching               | served while caching              | served while caching                |
| Partition response  | serve from upstream, cache locally | serve; converge when peer returns | serve; converge when remote returns |
| Retry               | reconstructible from upstream      | reconstructible from upstream     | reconstructible from upstream       |

A cache fill is the one mutation that never refuses on a peer partition. It is reconstructible: a copy lost with a
node's storage is refetched from the upstream on the next miss, so gating it on a peer would trade availability for a
durability the upstream already provides.

### OCI push: push a manifest or blob to a hosted registry

| Aspect              | `none`                 | `dc`                                   | `ha`                                              |
| ------------------- | ---------------------- | -------------------------------------- | ------------------------------------------------- |
| Durability evidence | manifest + blobs local | manifest + blobs durable in-DC         | manifest durable remote; blobs converge after ack |
| Client result       | pullable locally       | pullable after failover                | manifest resolvable; remote blobs may lag         |
| Partition response  | commit locally         | refuse with `503` until peer reachable | refuse with `503` until remote metadata reachable |
| Retry               | idempotent by digest   | idempotent by digest                   | idempotent by digest                              |

A manifest names its config and layer blobs by digest, so its metadata is durable before every blob it references is
remote, exactly as an upload's is. A pull that resolves the manifest but reaches a not-yet-converged blob is told the
blob is unavailable, not given wrong bytes.

### OCI delete: delete a manifest or tag

| Aspect              | `none`                           | `dc`                                   | `ha`                                     |
| ------------------- | -------------------------------- | -------------------------------------- | ---------------------------------------- |
| Durability evidence | metadata removed local           | removal durable in-DC                  | removal durable remote                   |
| Client result       | digest gone locally              | removal survives failover              | removal survives datacenter loss         |
| Partition response  | commit locally                   | refuse with `503` until peer reachable | refuse with `503` until remote reachable |
| Retry               | idempotent (absent stays absent) | idempotent (absent stays absent)       | idempotent (absent stays absent)         |

## Why a partition refuses instead of accepting

`dc` and `ha` promise durability in a failure domain their acknowledgement names. During a partition that cuts peryx off
from that domain, an authoritative mutation cannot make the promise true, so it **refuses** with
`503 Service Unavailable` rather than commit locally and return a success that lies. Accepting under those modes would
be the [dependency-confusion](@/core/glossary.md#shadowing) of durability: a green result the operator later discovers
was never safe.

{% mermaid() %}
flowchart TB
m["authoritative mutation"] --> mode{"mode"}
mode -->|none| local["commit local, ack"]
mode -->|dc| dcq{"in-DC peer reachable?"}
mode -->|ha| haq{"remote metadata reachable?"}
dcq -->|yes| dc_ack["commit + replicate, ack"]
dcq -->|no| refuse["503, refuse"]
haq -->|yes| ha_ack["commit metadata remote, ack; bytes converge"]
haq -->|no| refuse
class local,dc_ack,ha_ack good
class refuse warn
{% end %}

Reads do not refuse. A partitioned node keeps serving the state it holds, bounded by the frontier below, because a stale
read a client can reason about is more useful than an error. The contract is
[CP](https://en.wikipedia.org/wiki/CAP_theorem) for authoritative mutations and available for reads. A cache fill, being
reconstructible, also does not refuse.

## Crash versus storage loss

The contract distinguishes two failures an implementation must not conflate:

- **Process crash, storage intact.** In every mode, anything an acknowledgement covered is on durable local storage and
  survives the restart. `none`'s freshness-cache writes are deliberately non-durable, so a crash can drop a cached page
  and cost a refetch, but no acknowledged **mutation** is ever lost this way. A crash is not a data-loss event.
- **Storage loss.** The node's disk is gone. `none` loses every mutation since the last external backup, its single
  failure domain being the thing that failed. `dc` loses nothing acknowledged, because a second in-DC domain holds it.
  `ha` loses nothing acknowledged in metadata, and loses only artifact bytes that had not yet converged, which are
  identifiable by digest and, for a cache fill, refetchable.

An implementation that treats a crash and a storage loss as one event either under-promises `none` or over-promises the
recoverable case; the contract requires it to tell them apart.

## The frontier bounds staleness

Every mutation commits at a monotonic **serial**. The serial a node has durably applied is its **frontier**: the exact
prefix of history it reflects. A replica's staleness is bounded by naming its frontier, the highest serial it holds, not
by a wall-clock estimate of how many seconds behind it runs.

A frontier is a fact the node can prove; "about a second behind" is a guess that a stalled link silently invalidates. A
reader that knows a replica's frontier knows precisely which acknowledged mutations it can and cannot yet see, and a
`dc` or `ha` promise is stated as "durable at serial *n* in domain *d*", never as a duration. peryx already carries this
serial as the [change serial](@/core/architecture.md#the-metadata-store) its
[replication journal](@/core/high-availability.md) orders; the contract makes it the unit in which every staleness and
recovery-point promise is expressed.

## Recovery objectives

RPO and RTO follow from the acknowledgement and the frontier, not from a stopwatch:

| Mode   | RPO (data at risk on domain loss)                                   | RTO (return to service)                                              |
| ------ | ------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `none` | everything after the last external backup's serial                  | operator-driven restore and [promotion](@/core/high-availability.md) |
| `dc`   | zero acknowledged metadata and bytes within the DC                  | failover to the surviving in-DC domain                               |
| `ha`   | zero acknowledged metadata; only unconverged bytes, named by digest | failover to a surviving datacenter                                   |

RPO is a serial, not a duration: "no acknowledged mutation at or before frontier *n*" is the promise, so an
implementation is measured by which serials it can recover, not by how many seconds of clock time it can name.

## Benchmark method for mode budgets

Later issues set the performance budget each mode must meet. This section fixes the method so those budgets compare like
with like, extending the [core methodology](@/core/performance.md): open-loop load, an
[HdrHistogram](https://github.com/HdrHistogram/HdrHistogram) for exact percentiles, and a median over independent
rounds.

A durability promise costs latency and throughput, and the two are budgeted as **separate gates**: a mode that holds its
p99 while its throughput collapses has still regressed, and one gate must never be averaged into the other to hide it.
Beside each gate, a mode's benchmark reports **CPU, RSS, allocations, and disk I/O**, because the cost of durability
lands there, in the `fsync`, the replication write, and the bytes on the wire, and a latency that only held because
memory or I/O blew out is not a pass. The `none` path is the zero-durability baseline every stronger mode is measured
against, so a mode's budget is the marginal cost it adds over `none`, not an absolute number that hardware alone can
move.

## Related

- Operate the single-writer model that is today's `none`: [high availability](@/core/high-availability.md)
- Where mutations and the change serial live: [architecture](@/core/architecture.md)
- How latency and throughput are measured: [performance and methodology](@/core/performance.md)
- The terms used above: [glossary](@/core/glossary.md)
