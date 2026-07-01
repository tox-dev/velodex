# velox

A PyPI-compatible read-through cache, written in Rust. Point pip, uv, or any PEP 503/691 client at velox and it proxies
and permanently caches a real index (pypi.org by default, or a private Artifactory, GitLab, or devpi mirror), serving
artifacts from a content-addressed store on the next request.

velox is small and fast: an async server (axum/tokio), a pure-Rust embedded metadata store (redb), and a
content-addressed blob cache. It speaks the wire protocols pip and uv already use, so no client changes are needed
beyond the index URL.

## Status

Early, but the read-through cache works end to end: `uv pip install` and `pip install` resolve and install through velox
against pypi.org, and every artifact is cached content-addressed on disk.

Implemented so far:

- `serve` and `init` commands, configured by a TOML file plus a few operational flags.
- The [Simple Repository API][simple-api] at `/{index}/simple/` (project list) and `/{index}/simple/{project}/`
  (detail), negotiated between [PEP 691][pep691] JSON and [PEP 503][pep503] HTML, versioned per [PEP 629][pep629] and
  carrying the [PEP 700][pep700] `versions`/`size`/`upload-time` fields and [PEP 592][pep592] yank markers.
- File download with content-addressed caching and sha256 verification (the `#sha256=` fragment from [PEP 503][pep503]).
- [PEP 658][pep658]/[PEP 714][pep714] `.metadata` siblings: advertised, and served by fetching the upstream sibling,
  verifying it against the advertised digest, and caching it. The end-to-end tests confirm both pip and uv take this
  metadata-only fast path for resolution — asserted from velox's own request metrics, not assumed.
- Content negotiation with the upstream: velox asks for [PEP 691][pep691] JSON and uses it when the mirror offers it;
  only when a mirror serves [PEP 503][pep503] HTML alone (Artifactory, GitLab, static indexes) does velox parse the HTML
  and re-serve it as JSON downstream.
- Composable indexes: any number of mirrors (each with its own upstream and credentials), local hosted stores that
  accept uploads, and overlays that serve several layers under one URL with first-match-per-filename resolution — your
  uploaded file shadows the upstream one, everything else falls through. The default topology is a pypi.org mirror with
  a local store overlaid in front of it at `root/pypi`.
- Per-mirror authentication (Basic or Bearer), including the pypi.org `__token__` convention from the
  [`.pypirc` spec][pypirc], and a configurable cache freshness window.
- Uploads via the [legacy upload API][upload-api] over token-authenticated Basic auth: each distribution is stored
  content-addressed in the overlay's local layer and served back through the same simple API. The end-to-end tests
  confirm both twine and `uv publish` can upload and that the result installs and imports.
- Removal of uploaded distributions, both reversible and permanent: [PEP 592][pep592] yank/un-yank, and hard delete on
  indexes marked `volatile`. Deleting an overlaid file makes the upstream version visible again.
- Structured, leveled logging to stdout, a file, journald, or syslog.

Not yet built: the web UI, and distribution as `PyPI` wheels or standalone installers. [proposal.md](proposal.md) holds
the full design and the phased plan.

## Install

From source, with a Rust toolchain (the version is pinned in `rust-toolchain.toml`):

```shell
cargo build --release
# the binary is at target/release/velox
```

`pip install velox` and standalone installers are planned.

## Quick start

Initialize a data directory and start the server:

```shell
velox init --data-dir ./velox-data
velox serve --data-dir ./velox-data --port 4433
```

Then install through it. The built-in pypi.org mirror is exposed under the `root/pypi` index:

```shell
uv pip install --index-url http://127.0.0.1:4433/root/pypi/simple/ six
# or
pip install --index-url http://127.0.0.1:4433/root/pypi/simple/ six
```

On a miss velox fetches from the upstream, verifies and caches the artifact, and serves it. Later requests are served
from the cache without touching the upstream.

## Configuration

velox runs with sensible defaults and no config at all — `velox serve` mirrors pypi.org with a local upload store
overlaid in front of it at `root/pypi`. Everything else lives in one TOML file, passed with `--config`. Only the
operational settings you tend to vary per run are also exposed as flags, which override the file. Precedence is
`defaults < TOML file < flags`.

| Setting                   | Flag              | TOML key         | Default      |
| ------------------------- | ----------------- | ---------------- | ------------ |
| Bind host                 | `--host`          | `host`           | `127.0.0.1`  |
| Bind port                 | `--port`          | `port`           | `4433`       |
| Data directory            | `--data-dir`      | `data_dir`       | `velox-data` |
| Config file               | `--config` / `-c` | (n/a)            | (none)       |
| Cache freshness (seconds) | (file only)       | `cache_ttl_secs` | `1800`       |
| Indexes                   | (file only)       | `[[index]]`      | (see below)  |

### Indexes

Indexes come in three composable shapes, declared as `[[index]]` tables. This is the virtual-repository model of
Artifactory and Nexus combined with devpi's index inheritance:

- **mirror** — proxies and caches an upstream simple index, with optional credentials.
- **local** — a hosted store that accepts uploads. `upload_token` is the Basic-auth password an upload must present
  (omit it to disable uploads); `volatile = true` (the default) allows delete and overwrite.
- **overlay** — serves several other indexes under one route in order. Resolution is first-match per filename: your
  uploaded file shadows the same filename upstream, and everything else falls through. Uploads to the overlay land in
  its `upload` layer (defaulting to its first local layer). Overlays can layer other overlays.

Declaring any `[[index]]` replaces the default topology, which is equivalent to:

```toml
[[index]]
name = "pypi"
mirror = "https://pypi.org/simple/"

[[index]]
name = "local"
local = true # set upload_token = "<secret>" to enable uploads

[[index]]
name = "root/pypi"
layers = ["local", "pypi"]
upload = "local"
```

Every mirror can have its own overlay. A fuller example — two mirrors with different credentials, each with its own
private layer in front:

```toml
[[index]]
name = "pypi"
mirror = "https://pypi.org/simple/"

[[index]]
name = "corp"
mirror = "https://myco.jfrog.io/artifactory/api/pypi/pypi/simple/"
token = "<access-token>" # Bearer; takes precedence over username/password

[[index]]
name = "team-local"
upload_token = "<shared-upload-secret>"
volatile = true

[[index]]
name = "team"
route = "team/dev" # URL prefix; defaults to name
layers = ["team-local", "corp"]
upload = "team-local"

[[index]]
name = "oss"
layers = ["pypi"]

[log]
level = "info"
format = "pretty"
sink = "stdout"
```

Secrets live in this file, so keep it readable only by the velox user (`chmod 600`). velox handles upstreams that serve
only HTML: it parses their PEP 503 page and re-serves it to clients as JSON, so uv and pip get the modern format even
from an old mirror.

### Uploading, yanking, and deleting

Point twine or `uv publish` at an index that accepts uploads (any username; the token is the password):

```shell
twine upload --repository-url http://127.0.0.1:4433/root/pypi/ -u __token__ -p <secret> dist/*
uv publish --publish-url http://127.0.0.1:4433/root/pypi/ -u __token__ -p <secret> dist/*
```

Uploaded distributions can be yanked ([PEP 592][pep592], reversible — the file stays downloadable when pinned but
resolvers skip it) or deleted outright (only on `volatile` indexes; for an overlay the upstream version becomes visible
again). Both operate on the index's local layer and take the same Basic-auth token:

```shell
curl -X PUT    -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/yank  # yank one version
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/yank  # un-yank
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/      # delete one version
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/            # delete the whole project
```

## Logging

The log level comes from `--log-level {error,warn,info,debug,trace}` or the `level` key under `[log]` in the config
file, and can target a single module (for example `velox_upstream=debug`). `-v` raises it to debug and `-vv` to trace.
Output goes to one sink, selected with `--log-sink`:

- `stdout` (default), pretty for a terminal or JSON with `--log-format json`
- `file`, a daily-rotating file at `--log-file <path>`
- `journald` on Linux, or `syslog`

## Endpoints

Every configured index route serves the same surface (`{route}` below, for example `root/pypi`):

| Path                                              | Purpose                                      |
| ------------------------------------------------- | -------------------------------------------- |
| `GET /{route}/simple/`                            | Project list (JSON or HTML by `Accept`)      |
| `GET /{route}/simple/{project}/`                  | Project detail, merged across overlay layers |
| `GET /{route}/files/{sha256}/{filename}`          | Cached artifact                              |
| `GET /{route}/files/{sha256}/{filename}.metadata` | PEP 658 core metadata                        |
| `POST /{route}/`                                  | Upload (legacy API; twine and `uv publish`)  |
| `PUT /{route}/{project}/[{version}/]yank`         | Yank uploaded files (PEP 592)                |
| `DELETE /{route}/{project}/[{version}/]yank`      | Un-yank                                      |
| `DELETE /{route}/{project}/[{version}/]`          | Delete uploaded files (volatile only)        |
| `GET /+status`                                    | JSON health, identity, and index routes      |
| `GET /metrics`                                    | Prometheus metrics                           |

## Standards

velox targets the Python packaging interoperability standards a modern index and its clients rely on:

- [Simple Repository API][simple-api], the consolidated living spec (currently serving `meta.api-version` 1.1).
- [PEP 503][pep503], the HTML simple index and name normalization.
- [PEP 691][pep691], the JSON simple index and content negotiation.
- [PEP 629][pep629], simple API versioning.
- [PEP 700][pep700], the `versions`, `size`, and `upload-time` fields.
- [PEP 592][pep592], yanked releases.
- [PEP 658][pep658] and [PEP 714][pep714], the `.metadata` sibling, fetched from the upstream, verified, and served.
- [PEP 440][pep440], version identifiers and ordering.
- [PEP 508][pep508], dependency specifiers.
- [PEP 427][pep427] and [PEP 625][pep625], wheel and sdist filenames.
- [Core metadata][core-metadata], the `METADATA`/`PKG-INFO` fields.
- The [legacy upload API][upload-api] and [`.pypirc`][pypirc] authentication, for the upcoming upload path.

## Development

```shell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs' --fail-under-lines 100 --fail-under-functions 100
```

velox holds 100% line and function coverage. The design, conventions, and roadmap live in [proposal.md](proposal.md).

### End-to-end client tests

Beyond the unit suite, an end-to-end suite drives the real pip and uv clients against a spawned velox, guarding
downstream compatibility against regressions. It has two tiers, both gated behind Cargo features so they stay out of the
default run:

```shell
# hermetic: velox proxies a local fixture index (tiny real wheels), no network — fast and deterministic
cargo test -p velox --features e2e

# live: the same flows against the real pypi.org, to catch upstream drift
cargo test -p velox --features e2e-live -- e2e_live
```

Each test owns an isolated velox (and, for the hermetic tier, its own fixture upstream) on an ephemeral port, so the
suite runs in parallel. Installs are verified by importing the distribution in the target environment, and the PEP 658
fast path is proven from velox's `velox_metadata_requests_total` metric — observed at the server, not inferred from a
client exit code. twine upload coverage arrives with the upload API in a later phase.

## License

MIT.

[core-metadata]: https://packaging.python.org/en/latest/specifications/core-metadata/
[pep427]: https://packaging.python.org/en/latest/specifications/binary-distribution-format/
[pep440]: https://packaging.python.org/en/latest/specifications/version-specifiers/
[pep503]: https://peps.python.org/pep-0503/
[pep508]: https://packaging.python.org/en/latest/specifications/dependency-specifiers/
[pep592]: https://peps.python.org/pep-0592/
[pep625]: https://packaging.python.org/en/latest/specifications/source-distribution-format/
[pep629]: https://peps.python.org/pep-0629/
[pep658]: https://peps.python.org/pep-0658/
[pep691]: https://peps.python.org/pep-0691/
[pep700]: https://peps.python.org/pep-0700/
[pep714]: https://peps.python.org/pep-0714/
[pypirc]: https://packaging.python.org/en/latest/specifications/pypirc/
[simple-api]: https://packaging.python.org/en/latest/specifications/simple-repository-api/
[upload-api]: https://docs.pypi.org/api/upload/
