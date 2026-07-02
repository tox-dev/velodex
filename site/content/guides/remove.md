+++
title = "Yank and delete packages"
description = "Yank an uploaded release per PEP 592, or delete it outright."
weight = 4
+++

Both operations act on uploaded files in an index's local layer and take the same Basic-auth token as uploads. Files
served from a mirror are read-only; removing an uploaded file that shadowed an upstream one makes the upstream
version visible again.

## Yank (reversible)

Yanking marks the file per [PEP 592](https://peps.python.org/pep-0592/): resolvers skip it, but an installation
pinned to the exact version can still fetch it. Audit trails and pinned builds survive.

```shell
# yank one version
curl -X PUT -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/yank

# yank every file of the project
curl -X PUT -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/yank

# un-yank
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/yank
```

## Delete (permanent)

Deletion removes the file records from the index. It requires the local layer to be `volatile` (the default);
set `volatile = false` on release indexes you want immutable, and velox answers `403` instead.

```shell
# delete one version
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/1.2.0/

# delete the whole project
curl -X DELETE -u __token__:<secret> http://127.0.0.1:4433/root/pypi/mypkg/
```

The content-addressed blob stays on disk after a delete; another index or a re-upload with the same digest reuses
it. Responses are `200` with the number of files affected, or `404` when nothing matched.
