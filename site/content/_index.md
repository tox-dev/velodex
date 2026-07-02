+++
template = "index.html"
+++

- **Read-through cache** Proxies pypi.org or any [PEP 503/691](https://packaging.python.org/en/latest/specifications/simple-repository-api/) index, verifies each artifact against its sha256, and
  stores it content-addressed; repeat requests come from disk.
- **Composable indexes** Mirrors, local hosted stores, and overlays that serve several layers under one URL, with
  your uploads shadowing upstream files. Each mirror can have its own overlay.
- **Private hosting** twine and `uv publish` upload over the standard API; you can yank a distribution ([PEP 592](https://peps.python.org/pep-0592/)) or
  delete it, which un-shadows the upstream version.
- **Modern protocols** PEP 691 JSON with PEP 503 HTML fallback, PEP 700 fields, and the PEP 658/714 `.metadata` fast
  path pip and uv use to resolve without downloading wheels.
- **One small binary** Async Rust ([axum](https://github.com/tokio-rs/axum)/tokio), an embedded crash-safe store ([redb](https://www.redb.org/)), no external services, and a
  single TOML file for configuration.
- **Proven with real clients** The test suite drives actual pip, uv, and twine against a live velox and holds 100%
  line and function coverage.
