+++
title = "Observe the dropped gpg-sig"
description = "Front a static index that advertises signatures on two files, and watch peryx drop the gpg-sig marker on the one it content-addresses while keeping it on the pass-through file."
weight = 5
+++

In this tutorial you front a small static index whose files both advertise a GPG signature, and watch peryx drop the
`gpg-sig` marker on the file it content-addresses onto its own route while keeping it on the file it passes through. It
takes about ten minutes and shows the marker tracks one thing: whether the URL peryx hands out has a signature next to
it.

## Prerequisites

You need a peryx binary ([installation](@/core/installation.md) lists the channels), Python, and
[curl](https://curl.se/). Work in a scratch directory.

## Build a static index that advertises signatures

A [PEP 503](https://peps.python.org/pep-0503/) index is a directory of HTML pages. Build one project page listing two
files, both marked `data-gpg-sig="true"`: one anchor carries a `#sha256=` fragment, the other carries no hash. The hash
is the only difference that matters here.

```shell
mkdir -p static/demo
: > static/demo-1.0-py3-none-any.whl
: > static/demo-1.0.post1-py3-none-any.whl
sha=$(python3 -c "import hashlib; print(hashlib.sha256(open('static/demo-1.0-py3-none-any.whl','rb').read()).hexdigest())")
cat > static/demo/index.html <<EOF
<!DOCTYPE html>
<html><body>
<a href="../demo-1.0-py3-none-any.whl#sha256=$sha" data-gpg-sig="true">demo-1.0-py3-none-any.whl</a>
<a href="../demo-1.0.post1-py3-none-any.whl" data-gpg-sig="true">demo-1.0.post1-py3-none-any.whl</a>
</body></html>
EOF
```

Serve the directory on port 8000 and leave it running:

```shell
python3 -m http.server 8000 --directory static
```

`demo-1.0` has a `sha256`, so peryx can content-address it. `demo-1.0.post1` has none, so peryx cannot, and will leave
its URL alone.

## Front it with peryx

In a second terminal, point a cached index at the static server and start peryx:

```toml
# peryx.toml
[[index]] # cached: read-through cache of the static index
name = "static"
cached = "http://127.0.0.1:8000/"
```

```shell
peryx serve --config peryx.toml
```

peryx listens on `127.0.0.1:4433`. Use a third terminal for the fetches.

## Read the JSON simple page

Ask peryx for the project page as JSON, the form `pip` and `uv` read:

```shell
curl -s -H "Accept: application/vnd.pypi.simple.v1+json" \
    http://127.0.0.1:4433/static/simple/demo/ | python3 -m json.tool
```

Look at the two file objects. The content-addressed file and the pass-through file diverge on both `url` and `gpg-sig`:

```json
{
  "filename": "demo-1.0-py3-none-any.whl",
  "url": "/static/files/e3b0c442.../demo-1.0-py3-none-any.whl"
}
```

```json
{
  "filename": "demo-1.0.post1-py3-none-any.whl",
  "url": "http://127.0.0.1:8000/demo-1.0.post1-py3-none-any.whl",
  "gpg-sig": true
}
```

`demo-1.0` had a `sha256`, so peryx rewrote its `url` to its own `/static/files/...` route and dropped the `gpg-sig`
field: the field is gone, not `false`. `demo-1.0.post1` had no hash, so peryx left its `url` pointing upstream and kept
`gpg-sig: true`. The upstream `.asc` is still next to that upstream URL, so the marker is still true there.

## Confirm the HTML form agrees

The same split shows in the [PEP 503](https://peps.python.org/pep-0503/) HTML page. Fetch it and read the two anchors:

```shell
curl -s http://127.0.0.1:4433/static/simple/demo/
```

The `demo-1.0` anchor points at `/static/files/...` and carries no `data-gpg-sig`; the `demo-1.0.post1` anchor keeps its
upstream `href` and its `data-gpg-sig="true"`. Both serving surfaces agree, because both clear the marker on the same
condition.

## What you saw

peryx drops the `gpg-sig` marker for a file it content-addresses onto its own route, where it serves the blob and the
`.metadata` sibling but never a `.asc`. It keeps the marker for a file it passes through at its upstream URL, where the
`.asc` is still reachable. The marker follows the file URL peryx hands out, nothing else.

## Where next

- The exact rule across JSON, HTML, and legacy JSON: [the gpg-sig marker](@/ecosystems/pypi/reference/gpg-sig.md)
- Why peryx drops it rather than serve the signature: [GPG signatures through peryx](@/ecosystems/pypi/gpg-sig.md)
- Move a tool off the marker: [rely on hashes, not gpg-sig](@/ecosystems/pypi/guides/gpg-sig.md)
