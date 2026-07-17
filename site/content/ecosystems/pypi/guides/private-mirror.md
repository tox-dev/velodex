+++
title = "Proxy a private upstream"
description = "Point peryx at Artifactory, GitLab, or any other PEP 503 index, with credentials."
weight = 3
+++

Declare a cached index whose `cached` field is the upstream's simple-index URL. Two authentication styles cover the
common servers; a bearer token wins when you set both.

## Artifactory or GitLab (bearer token)

```toml
[[index]]
name = "corp"
cached = "https://myco.jfrog.io/artifactory/api/pypi/pypi/simple/"
token = "<access-token>"
```

## pypi.org-style Basic auth

pypi.org tokens use the `__token__` username convention from the
[`.pypirc` specification](https://packaging.python.org/en/latest/specifications/pypirc/):

```toml
[[index]]
name = "corp"
cached = "https://private.example/simple/"
username = "__token__"
password = "<token>"
```

Start peryx with `--config` and install through `http://<host>:<port>/corp/simple/`.

## Read Basic credentials from netrc

One opt-in netrc file can hold Basic credentials outside `peryx.toml`. This uses the same `machine`, `login`, and
`password` form as [pip](https://pip.pypa.io/en/stable/topics/authentication/#netrc-support):

```toml
netrc = "/run/secrets/upstream.netrc"

[[index]]
name = "corp"
cached = "https://private.example/simple/"
```

```text
machine private.example
login __token__
password pypi-token
```

Run `chmod 600 /run/secrets/upstream.netrc` on Unix. A `token`, or a complete `username` and `password` pair on the
index, overrides netrc. The [configuration reference](@/core/configuration.md#upstream-netrc-credentials) covers custom
ports, `default` entries, startup errors, and redirect isolation.

## Sync for offline use

`peryx mirror sync` uses the same upstream URL and credentials as serving. Configure the working set next to the cached
index, then populate and verify the cache while the upstream is reachable:

```toml
[[index]]
name = "corp"
cached = "https://private.example/simple/"
token = "<token>"

[index.prefetch]
requirements = ["requirements.txt"]
```

```shell
peryx mirror sync corp --config peryx.toml
peryx mirror verify corp --config peryx.toml
peryx serve --config peryx.toml --offline
```

Set `offline = true` on the cached index when only that upstream should stay cache-only. Use the top-level
`offline = true` or `serve --offline` when every cached index in the process must avoid network access.

### Sync every project name

Set `mode = "all"` when the mirror must discover every project exposed by the upstream root rather than a configured
working set:

```toml
[index.prefetch]
mode = "all"
```

The sync negotiates the
[PyPA Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/) and
[PEP 691](https://peps.python.org/pep-0691/) JSON root first, then accepts the
[PEP 503](https://peps.python.org/pep-0503/) HTML form. It records project names only. Project pages, release metadata,
artifact files, and metadata siblings remain subject to the prefetch filters and their normal fetches. Warehouse's
[root implementation](https://github.com/pypi/warehouse/blob/main/warehouse/api/simple.py) establishes the production
shape this path targets: display names, canonical links, a last-serial extension, and a root large enough to require
streaming.

The root transfer completes into a temporary file before peryx changes catalog metadata. Parsing then writes batches of
10,000 canonical/display-name pairs into a staging generation. A single pointer change publishes that generation after
the parser reaches a valid end of document. A truncated transfer, malformed document, unsupported Simple API major,
invalid name, or failed batch leaves the previous generation active. The next sync removes abandoned staging and retired
generations in bounded batches. The server's persistent `writer_identity` claim provides cross-process single-writer
exclusion; concurrent sync calls within one process share a per-index lock and one fetch. This keeps the failure
behavior used by devpi's [`ProjectNamesCache`](https://github.com/devpi/devpi/blob/main/server/devpi_server/mirror.py),
which retains its previous name set when refresh fails, while using the durable progress discipline in
[bandersnatch's mirror](https://github.com/pypa/bandersnatch/blob/main/src/bandersnatch/mirror.py), which does not
advance the completed serial after an errored synchronization. Peryx's generation pointer applies those rules to batched
root parsing: durable staging work may be discarded, but readers see only the last complete generation.

Peryx sends `If-None-Match` on the next sync when the upstream supplied an ETag. `If-Modified-Since` is the fallback
when only `Last-Modified` is available, matching the precedence in
[HTTP conditional requests](https://www.rfc-editor.org/rfc/rfc9110.html#name-preconditions). A `304 Not Modified` keeps
the generation and name rows, updates the fetch time, and merges only validator headers present on the response, as
[HTTP cache validation](https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4) requires. Validators belong to the
configured upstream source, so a routed fallback never receives another source's validator.

The decompressed root is limited to 256 MiB and 2,000,000 entries. In July 2026, Warehouse's JSON and HTML roots are
about 42–44 MiB and list fewer than one million projects, leaving substantial growth room while bounding local disk,
parser work, and recovery. The existing redirect policy permits at most ten redirects. Persisted source and final URLs
strip user information, query strings, and fragments.

`/metrics` reports `peryx_catalog_syncs_total`, `peryx_catalog_published_total`, `peryx_catalog_not_modified_total`,
`peryx_catalog_errors_total`, and the `peryx_catalog_projects` gauge. These series use only the bounded `ecosystem` and
`role` labels; upstream names, URLs, index names, and project names never become Prometheus labels.

## HTML upstreams

Some upstreams, including [Artifactory](https://jfrog.com/artifactory/), serve the
[PEP 503](https://peps.python.org/pep-0503/) HTML form instead of PEP 691 JSON. peryx requests
[PEP 691](https://peps.python.org/pep-0691/) JSON first, parses HTML when the upstream returns it, and serves JSON to
[pip](https://pip.pypa.io/) and [uv](https://docs.astral.sh/uv/). You do not configure this; it happens per response.
The upstream response must send a Simple API content type (`text/html`, `application/vnd.pypi.simple.v1+html`, or
`application/vnd.pypi.simple.v1+json`); other content types return `502` with the upstream URL in the error body.

## Notes

- Inline credentials make the config file secret, so restrict it: `chmod 600 peryx.toml`.
- Each cached index keeps its own credentials. A cached file remembers which cached index it came from, and a later
  cache-miss fetch reuses that index's authentication.
- Peryx asks upstream for `Accept-Encoding: identity` during artifact downloads. This makes the bytes pip and uv verify
  match the cached bytes. Same-origin redirects keep the cached index's credentials; cross-origin requests do not.
- `cache_ttl_secs` (default 1800) controls how long peryx serves a cached project page before revalidating it against
  the upstream with `If-None-Match`.
- Peryx caches upstream `404` misses for project pages and `.metadata` siblings for 30 seconds.

## Related

- Why one URL with shadowing beats `--extra-index-url`: [the index model](@/core/indexes.md)
- Serve a network with no internet route: [air-gapped](@/ecosystems/pypi/guides/air-gapped.md)
- Upstream capability differences peryx papers over: [standards](@/ecosystems/pypi/reference/standards.md)
