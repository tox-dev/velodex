+++
title = "Standards"
description = "The packaging PEPs and specifications velodex implements, and how they fit together."
weight = 3
+++

velodex targets the interoperability standards a modern index and its clients rely on. The
[Simple Repository API](https://packaging.python.org/en/latest/specifications/simple-repository-api/) is the living
consolidation of most of them; velodex serves `meta.api-version` 1.1.

| Standard | Role in velodex |
| -------- | ------------- |
| [PEP 503](https://peps.python.org/pep-0503/) | The HTML simple index and project-name normalization; served to clients that do not ask for JSON, and parsed from HTML-only upstreams |
| [PEP 691](https://peps.python.org/pep-0691/) | The JSON simple index and its content negotiation; the primary wire format both directions |
| [PEP 629](https://peps.python.org/pep-0629/) | Version marker on responses so clients can detect capabilities |
| [PEP 700](https://peps.python.org/pep-0700/) | The `versions`, `size`, and `upload-time` fields of api-version 1.1 |
| [PEP 592](https://peps.python.org/pep-0592/) | Yanked files: parsed from upstreams, re-served, and settable on uploads |
| [PEP 658](https://peps.python.org/pep-0658/) / [PEP 714](https://peps.python.org/pep-0714/) | The `.metadata` sibling that lets resolvers skip wheel downloads; advertised, fetched, verified, and cached |
| [PEP 440](https://packaging.python.org/en/latest/specifications/version-specifiers/) | Version parsing and ordering |
| [PEP 427](https://packaging.python.org/en/latest/specifications/binary-distribution-format/) / [PEP 625](https://packaging.python.org/en/latest/specifications/source-distribution-format/) | Wheel and sdist filename handling |
| [Core metadata](https://packaging.python.org/en/latest/specifications/core-metadata/) | The `METADATA` document served as the PEP 658 sibling |
| [Legacy upload API](https://docs.pypi.org/api/upload/) | The multipart upload protocol twine and `uv publish` speak |
| [`.pypirc`](https://packaging.python.org/en/latest/specifications/pypirc/) | The `__token__` authentication convention for uploads and upstream mirrors |

## PEP 714 and the `core-metadata` key

PEP 658 shipped with a bug in its `dist-info-metadata` key name, and PEP 714 renamed it to `core-metadata`. Indexes
such as pypi.org emit both keys for compatibility. velodex reads only `core-metadata` and ignores the legacy key,
because accepting both as aliases would make a strict parser reject the duplicate; downstream it emits both HTML
attributes for older clients, matching pypi.org's behavior.

## Graceful degradation

Some upstreams implement only part of the stack; Artifactory and GitLab serve HTML alone. velodex negotiates
JSON first, parses PEP 503 HTML as the fallback, and re-serves the modern formats downstream, so a client
gets api-version 1.1 with PEP 700 fields regardless of what the upstream offered. Features the upstream cannot
express (a missing `.metadata` sibling, absent sizes) degrade per file rather than per index.
