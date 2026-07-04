+++
title = "Capability matrix"
description = "Which roles and features velodex supports per ecosystem. PyPI ships today; OCI and npm are planned."
weight = 5
+++

velodex is a `(role × ecosystem)` server: every index is one of three [roles](@/reference/glossary.md#roles) paired with
an [ecosystem](@/reference/glossary.md#ecosystem). This page is the coverage map. `✓` means shipping today; **planned**
means the architecture reserves it but no driver is built yet.

## Roles per ecosystem

| Role                                              | PyPI | OCI     | npm     |
| ------------------------------------------------- | ---- | ------- | ------- |
| [cached](@/reference/glossary.md#roles) (proxy)   | ✓    | planned | planned |
| [hosted](@/reference/glossary.md#roles) (uploads) | ✓    | planned | planned |
| [virtual](@/reference/glossary.md#roles) (layers) | ✓    | planned | planned |

## Features per ecosystem

| Feature                                                          | PyPI | OCI     | npm     |
| ---------------------------------------------------------------- | ---- | ------- | ------- |
| Read-through caching with streaming                              | ✓    | planned | planned |
| Content-addressed artifact store                                 | ✓    | planned | planned |
| [Shadowing](@/reference/glossary.md#shadowing) / virtual resolve | ✓    | planned | planned |
| Publish / upload API                                             | ✓    | planned | planned |
| Yank and delete                                                  | ✓    | planned | planned |
| Metadata fast path (PEP 658/714)                                 | ✓    | n/a     | n/a     |
| `velodex mirror` sync + offline                                  | ✓    | planned | planned |
| Policy filters (allow/deny, versions, size)                      | ✓    | planned | planned |
| Signed webhooks                                                  | ✓    | planned | planned |
| Web UI, search, archive inspection                               | ✓    | planned | planned |

Rows marked `n/a` are PyPI-specific protocol features that do not translate to another ecosystem; the OCI and npm
columns will grow their own equivalents. Cross-cutting subsystems (metrics, rate limits, logging, backup/restore) are
ecosystem-neutral and apply to every index regardless of role or ecosystem.

## Related

- What the roles mean: [the index model](@/explanation/indexes.md)
- Per-ecosystem setup: [ecosystems](@/ecosystems/_index.md)
- The terms in one place: [glossary](@/reference/glossary.md)
