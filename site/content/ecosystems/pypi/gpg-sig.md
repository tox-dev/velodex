+++
title = "GPG signatures through peryx"
description = "Why peryx stops advertising a file's gpg-sig marker once it content-addresses the file onto its own route, and the 404 that dropping the marker prevents."
weight = 6
+++

peryx no longer advertises a GPG signature for the files it content-addresses onto its own route. This page explains why
serving the blob without the signature forces that choice, and the client failure it heads off.

## What peryx serves for a file

When peryx content-addresses an upstream file, it rewrites the file URL to its own `/{route}/files/{sha256}/{filename}`
route and serves the file from there. Under that route it serves two things: the artifact blob, and the
[PEP 658](https://peps.python.org/pep-0658/) `.metadata` sibling that lets a resolver read dependency metadata without
downloading the whole wheel. It does not serve the detached OpenPGP signature, the `.asc` sibling that
[PEP 503](https://peps.python.org/pep-0503/) places next to the file URL. That signature only ever existed at the
upstream URL, and peryx has replaced that URL with its own.

The `gpg-sig` marker (`data-gpg-sig` in HTML, `has_sig` in the legacy JSON) is a promise about the file URL: it says an
`.asc` is reachable at `{file_url}.asc`. Upstream, the marker rode along with the file record when peryx rewrote the
URL, so peryx kept advertising a signature at a route where none exists.

## The failure it prevents

A client that trusts the marker does the obvious thing: it fetches `{file_url}.asc` to get the signature. Before this
change, that URL was peryx's own file route, where no `.asc` is served, so the client got a `404`. The marker named a
file that was not there.

Two ways make the page honest again. peryx could fetch and cache the upstream `.asc` and serve it next to the blob, the
way it serves the `.metadata` sibling. Or it could drop the marker whenever it rewrites the URL, so it never promises a
signature it will not serve. peryx takes the second:
[PyPI deprecated GPG signatures in 2023](https://blog.pypi.org/posts/2023-05-23-removing-pgp/) and stopped serving them,
so wiring up a whole fetch-and-serve path for a signature the ecosystem is retiring would be effort spent on a dead
surface. Dropping a marker peryx cannot back is the smaller fix.

## Where the marker survives

The marker is not gone from peryx. A file peryx serves at its **upstream URL** unchanged, a pass-through, keeps it,
because the upstream `.asc` is still reachable next to that URL. Pass-through happens when peryx has no `sha256` to
content-address the file by and so leaves the URL alone. There the marker is still true, so peryx passes it through
untouched. The marker tracks one fact only: whether the URL peryx hands out has a signature next to it.

## In practice

- The exact rule across JSON, HTML, and legacy JSON: [the gpg-sig marker](@/ecosystems/pypi/reference/gpg-sig.md)
- What to do if a tool relied on the marker or the `.asc`:
  [rely on hashes, not gpg-sig](@/ecosystems/pypi/guides/gpg-sig.md)
- Watch it happen on two files: [observe the dropped gpg-sig](@/ecosystems/pypi/tutorials/gpg-sig.md)
