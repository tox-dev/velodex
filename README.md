# velox

A PyPI-compatible read-through cache and private-index overlay, written in Rust.

velox proxies and caches pypi.org (or any PEP 503 index), lets teams publish private packages into overlay indexes
that shadow the upstream mirror, and serves pip, uv, twine, Poetry, hatch, and flit over the standard wire protocols.
It targets low resource use and high throughput through async Rust, content-addressed copy-on-write storage, and
responses precomputed at index-update time.

## Design

[proposal.md](proposal.md) holds the full design: the standards and endpoint surface, the overlay/SRO index model,
the storage and filesystem cache, the tolerant upstream adapter for non-feature-complete indexes, the upload protocol,
auth, the web UI, the Rust stack, distribution, a 100% test and pip/uv conformance strategy, and a six-phase
implementation plan delivered in one PR.

## Status

Design proposal. Implementation lands as a pull request.
