+++
title = "Ecosystems"
description = "Which packaging formats velodex serves. PyPI today; OCI and npm are planned. Every index is a role paired with an ecosystem."
sort_by = "weight"
template = "section.html"
+++

An **ecosystem** is a packaging format and the wire protocol that carries it: how clients ask for packages, how versions
and filenames are shaped, and what an "artifact" (an installable file) looks like. PyPI is one ecosystem; OCI (container
images) and npm (JavaScript) are others.

velodex treats the ecosystem as a first-class axis. Every index you configure is a **role** (what it does — cached,
hosted, or virtual; see [the index model](@/explanation/indexes.md)) paired with an **ecosystem** (which format it
speaks). The two are independent: the same three roles work for every ecosystem, and a
[virtual index](@/reference/glossary.md#roles) may only combine members of the same ecosystem.

Today velodex ships one ecosystem, PyPI. The architecture is built so more plug in without reshaping the core — each new
ecosystem is a driver that teaches velodex that format's protocol and artifact rules. The
[capability matrix](@/reference/capabilities.md) tracks what each ecosystem supports.

Pick your ecosystem below for its "Set Me Up" hub: what the role trio means for that format, the wire protocol, and the
client-config snippets to point your tools at velodex.
