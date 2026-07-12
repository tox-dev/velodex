+++
title = "Rely on hashes, not gpg-sig"
description = "Move a client or tool that read the gpg-sig marker or fetched a file's .asc off it: verify with the sha256 the index already serves, and reach the origin index if a signature is still required."
weight = 8
+++

You have a client or a tool that read the `gpg-sig` marker (`data-gpg-sig` in HTML, `has_sig` in the legacy JSON) or
fetched a file's `.asc` signature. Through peryx that marker is now absent for the files peryx content-addresses onto
its own route, and `{file_url}.asc` on those files returns `404`. This guide covers what to rely on instead.

## Stop fetching `.asc` from peryx's file route

If your tool sees the marker and fetches `{file_url}.asc`, drop that fetch for any file whose URL points at peryx's
`/{route}/files/{sha256}/{filename}` route. peryx serves the blob and the [PEP 658](https://peps.python.org/pep-0658/)
`.metadata` sibling there, never the `.asc`. The signature was never at peryx's route; it lived next to the upstream URL
that peryx replaced. The marker is now dropped so your tool does not chase a `404`.

## Verify with the hash the index serves

Integrity is what most `.asc` checks were after, and the Simple API already carries it. Every file object lists a
`hashes` map, and peryx serves a `sha256` for each content-addressed file:

```shell
curl -s -H "Accept: application/vnd.pypi.simple.v1+json" \
    http://127.0.0.1:4433/root/pypi/simple/requests/ \
    | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['files'][0]['hashes'])"
```

`pip` and `uv` already verify this hash on download; peryx verifies the digest as it streams the blob into its store. A
lockfile pins the same `sha256`, so requiring hashes in your install (`pip install --require-hashes`, a `uv.lock`, or
`--only-binary` with pinned digests) gives you tamper-evidence tied to the file bytes, without a signature.

## When the signature is required

If a policy requires the OpenPGP signature, fetch it from the origin index that holds it, not through peryx. Read the
original file URL from the upstream index's own Simple page and fetch its `.asc` there:

```shell
curl -sfO https://files.pythonhosted.org/.../requests-2.32.5-py3-none-any.whl.asc
```

Two caveats. [PyPI deprecated GPG signatures in 2023](https://blog.pypi.org/posts/2023-05-23-removing-pgp/) and no
longer serves them, so for files from pypi.org the `.asc` is gone at the source too, not only through peryx. And a
private upstream that still signs is reachable only if your network allows it; the point of peryx is often that it does
not. Weigh whether a deprecated signature is worth a direct dependency on the origin before you build it in.

## Related

- The exact rule for when peryx keeps versus drops the marker:
  [the gpg-sig marker](@/ecosystems/pypi/reference/gpg-sig.md)
- Why peryx drops it: [GPG signatures through peryx](@/ecosystems/pypi/gpg-sig.md)
- See it drop on one file and survive on another: [observe the dropped gpg-sig](@/ecosystems/pypi/tutorials/gpg-sig.md)
