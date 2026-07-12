+++
title = "The gpg-sig marker"
description = "When peryx advertises a file's GPG signature marker and when it drops it: kept for pass-through upstream URLs, dropped for the files peryx content-addresses onto its own route."
weight = 4
+++

The Simple API can mark a file as having a detached OpenPGP signature next to it.
[PEP 503](https://peps.python.org/pep-0503/) spells the marker `data-gpg-sig` on the HTML anchor,
[PEP 691](https://peps.python.org/pep-0691/) spells it `gpg-sig` on the JSON file object, and the legacy PyPI JSON API
spells it `has_sig`. All three mean the same thing: a signature is served as an `.asc` sibling of the file URL, at
`{file_url}.asc`.

## When peryx keeps it

peryx keeps the marker when it serves a file at its **upstream URL** unchanged, a pass-through. That happens when peryx
has no `sha256` to content-address the file by, so it does not rewrite the URL. The upstream `.asc` sits next to the
upstream file, which is still where the file URL points, so the marker stays true and peryx passes it through.

## When peryx drops it

peryx drops the marker when it **content-addresses** the file, rewriting the file URL to its own
`/{route}/files/{sha256}/{filename}` route (see [endpoints](@/ecosystems/pypi/reference/endpoints.md)). At that route
peryx serves the blob and the [PEP 658](https://peps.python.org/pep-0658/) `.metadata` sibling, and nothing else. There
is no `.asc` there, so peryx clears the marker rather than advertise a signature it will not serve. A file carries a
`sha256` in almost every real index, so this is the common case.

The rule holds across all three surfaces, and both serving paths agree on it:

| Surface          | Marker         | Content-addressed file | Pass-through file |
| ---------------- | -------------- | ---------------------- | ----------------- |
| PEP 691 JSON     | `gpg-sig`      | omitted                | passed through    |
| PEP 503 HTML     | `data-gpg-sig` | omitted                | passed through    |
| Legacy PyPI JSON | `has_sig`      | `false`                | reflects upstream |

The JSON simple API served to `pip` and `uv` streams through one transformer; the HTML page and the legacy JSON are
rendered from the buffered resolve path. Both clear the marker on the same condition, so a file reads the same way
whichever surface a client asks for.

## What peryx does not serve

For a content-addressed file, peryx serves exactly two things under its file route: the artifact blob at
`/{route}/files/{sha256}/{filename}`, and its core-metadata at `.../{filename}.metadata`. It does **not** serve an
`.asc` at `.../{filename}.asc`; that route returns `404`. The detached signature only ever lived at the upstream URL,
which peryx has replaced with its own for a content-addressed file, so dropping the marker keeps the page honest about
what is reachable.

## In practice

- Why peryx drops the marker rather than serve the signature:
  [GPG signatures through peryx](@/ecosystems/pypi/gpg-sig.md)
- What a client that read the marker should do now: [rely on hashes, not gpg-sig](@/ecosystems/pypi/guides/gpg-sig.md)
- Watch peryx drop and keep the marker on two files:
  [observe the dropped gpg-sig](@/ecosystems/pypi/tutorials/gpg-sig.md)
