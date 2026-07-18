+++
title = "High availability"
description = "Run one writer with read replicas and promote a replica during a planned failover."
weight = 7
+++

peryx supports one writer with multiple read replicas. Send mutation traffic to the writer. Replicas serve data copied
from the writer and reject mutation requests with `503 Service Unavailable`.

This page operates the `none` [availability contract](@/core/availability-contracts.md): peryx provides local durability
and leaves copying and failover to you. That contract also defines the `dc` and `ha` modes later work adds, and the
normative meaning of every acknowledgement below.

Give every writer a distinct, stable identity:

```toml
writer_identity = "writer-a"
```

At startup, the writer claims that identity in the metadata store. A writer configured with another identity cannot open
the store until an operator promotes it. This prevents a restored copy from starting as a second writer.

Enable replica mode in TOML:

```toml
read_only = true
```

A replica must retain the writer's identity in its configuration, and the copied metadata store must contain the same
claim. peryx stops startup unless that claim matches a nonblank configuration value.

```shell
PERYX_READ_ONLY=true peryx serve --config peryx.toml
peryx serve --config peryx.toml --read-only
```

The environment variable and command-line flag provide the same setting. Replica mode does not claim the configured
writer identity, so a restored configuration may retain the source writer's identity. It disables upstream cache fills,
webhook delivery, and background maintenance. A node that follows a primary through the
[`[availability]`](@/core/configuration.md#availability) table's `replica` role enforces the same writer-identity check.

Populate each replica's data directory from a verified backup or an external replication system before routing traffic
to it. Copy the metadata store and referenced blobs from the same point in time. peryx does not copy data between nodes
or coordinate a shared blob store.

## Load-balancer probes

`GET /+health` is the liveness probe. It returns `200 OK` with `{"status":"live"}` while the HTTP process can answer.
Metadata, blob-store, and upstream failures do not fail liveness because a restart cannot repair those dependencies.

`GET /+ready` checks the local metadata store and blob-store root used by package requests. It returns `200 OK` with
`{"status":"ready"}` or `503 Service Unavailable` with `{"status":"not_ready"}`. It does not scan metadata, enumerate
repositories, or contact an upstream. `GET /+ready?writes=true` also requires a writer; replicas return `503` for that
query while remaining ready for reads.

Both public probes are anonymous, bypass the hosted request limiter, and send `Cache-Control: no-store`. Their fixed
documents contain no repository, upstream, user, topology, or failure details. `GET /+status` remains the detailed
operator surface. It reports the process role, local-store state, and last observed upstream reachability. Restrict
`/+status` at the ingress when that topology is sensitive.

For [Kubernetes probes](https://kubernetes.io/docs/concepts/workloads/pods/probes/), let readiness remove a pod from
service before liveness restarts it:

```yaml
livenessProbe:
  httpGet:
    path: /+health
    port: 4433
  periodSeconds: 10
  failureThreshold: 3
readinessProbe:
  httpGet:
    path: /+ready
    port: 4433
  periodSeconds: 5
  failureThreshold: 2
```

A generic load balancer should use readiness to select backends. For example, an
[HAProxy HTTP health check](https://www.haproxy.com/documentation/haproxy-configuration-tutorials/reliability/health-checks/)
can use the same route for a read pool:

```haproxy
backend peryx-readers
    option httpchk GET /+ready
    http-check expect status 200
    server peryx-1 10.0.0.11:4433 check
    server peryx-2 10.0.0.12:4433 check
```

Use `/+ready?writes=true` for the writer pool. Do not use `/+health` for load balancing because it detects a process
that cannot answer at all, so it remains successful during recoverable dependency failures.

## Manual promotion

1. Stop or fence the old writer so it cannot accept another mutation.

1. Finish copying its metadata and blobs to the selected replica and verify the copy.

1. With the replica stopped and still configured with the old identity, replace the store's writer claim:

   ```shell
   peryx writer promote writer-b --config peryx.toml
   ```

   The command compares the configured identity with the store's current claim and refuses a stale or missing value.

1. Set `writer_identity = "writer-b"`, remove replica mode, and start the selected replica.

1. Wait for `GET /+ready?writes=true` to return `200`, then move write traffic to it.

1. Rebuild former writer nodes as replicas before returning them to service.

Promotion changes the store's claim; it does not copy data or stop the old process. peryx does not provide leader
election or online promotion. Do not promote until you fence the old writer, and do not start two writers against copies
that can diverge.
