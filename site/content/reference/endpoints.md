+++
title = "HTTP endpoints"
description = "The routes every index serves, plus health and metrics."
weight = 2
+++

Every configured index route serves the same surface; `{route}` below is the index's `route`, for example
`root/pypi`. velox resolves a request to the index with the longest matching route prefix. The
[API explorer](@/reference/api.md) breaks each endpoint down with copyable example requests and responses.

| Method and path                                    | Purpose                                              |
| -------------------------------------------------- | ---------------------------------------------------- |
| `GET /{route}/simple/`                              | Project list, JSON or HTML by `Accept`               |
| `GET /{route}/simple/{project}/`                    | Project detail, merged across overlay layers         |
| `GET /{route}/files/{sha256}/{filename}`            | Artifact download, cached content-addressed          |
| `GET /{route}/files/{sha256}/{filename}.metadata`   | [PEP 658](https://peps.python.org/pep-0658/) core-metadata sibling                        |
| `POST /{route}/`                                    | Upload ([legacy API](https://docs.pypi.org/api/upload/), used by twine and `uv publish`)  |
| `PUT /{route}/{project}/[{version}/]yank`           | Yank uploaded files ([PEP 592](https://peps.python.org/pep-0592/))                        |
| `DELETE /{route}/{project}/[{version}/]yank`        | Un-yank                                              |
| `DELETE /{route}/{project}/[{version}/]`            | Delete uploaded files (volatile local layers only)   |
| `GET /+status`                                      | JSON health: version, index routes, serial           |
| `GET /metrics`                                      | [Prometheus](https://prometheus.io/docs/instrumenting/exposition_formats/) text exposition                           |

## Content negotiation

Simple-API responses honor the `Accept` header: `application/vnd.pypi.simple.v1+json` ([PEP 691](https://peps.python.org/pep-0691/)) when the client asks
for JSON, `text/html` ([PEP 503](https://peps.python.org/pep-0503/)) otherwise. Responses carry `Vary: Accept` and advertise `meta.api-version` 1.1,
which includes the [PEP 700](https://peps.python.org/pep-0700/) `versions`, `size`, and `upload-time` fields.

## Authentication

`POST`, `PUT`, and `DELETE` require `Authorization: Basic` where the password is the target local index's
`upload_token`; the username is ignored. Responses:

| Status | Meaning                                                       |
| ------ | ------------------------------------------------------------- |
| `200`  | Accepted; removal responses state how many files were affected |
| `400`  | Malformed upload: wrong `:action`, missing field, digest mismatch |
| `401`  | Missing or wrong token                                         |
| `403`  | Uploads disabled (no token configured) or index not volatile   |
| `404`  | Unknown route, project, or nothing matched                     |
| `405`  | The route's index does not accept writes                       |

## Metrics

`GET /metrics` exposes Prometheus counters:

- `velox_requests_total`: HTTP requests served.
- `velox_metadata_requests_total`: PEP 658 `.metadata` siblings served; a rising value proves clients resolve via
  the metadata fast path rather than by downloading wheels.
