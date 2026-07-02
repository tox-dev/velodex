+++
title = "Command line"
description = "The velodex binary's commands and flags."
weight = 4
+++

```
velodex [OPTIONS] <COMMAND>
```

## Commands

| Command | Purpose                                                        |
| ------- | -------------------------------------------------------------- |
| `serve` | Run the server                                                  |
| `init`  | Create the data directory and its stores, then exit             |

## Options

| Flag                | Meaning                                                   | Default       |
| ------------------- | ---------------------------------------------------------- | ------------- |
| `--host <addr>`     | Bind address                                               | `127.0.0.1`   |
| `--port <port>`     | Bind port                                                  | `4433`        |
| `--data-dir <path>` | Data directory (redb store and blob cache)                 | `velodex-data`  |
| `--config <path>`   | TOML configuration file                                    | (none)        |
| `--log-level <dir>` | [`tracing` directive](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html): `error`, `warn`, `info`, `debug`, `trace`, or per-module | `info` |
| `-v`, `-vv`         | Raise the level to debug / trace                           |               |
| `--log-format <f>`  | `pretty` or `json`                                         | `pretty`      |
| `--log-sink <s>`    | `stdout`, `file`, `journald`, `syslog`                     | `stdout`      |
| `--log-file <path>` | Log file path, required with `--log-sink file`             | (none)        |

Flags override the config file; see [Configuration](@/reference/configuration.md) for the full precedence and the
`[[index]]` schema.
