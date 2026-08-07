# next-cli

`bottles-cli`, the command-line interface for Bottles Next. It links
[`next-core`](../next-core) directly (no server/IPC involved) and drives all
bottle management from the terminal.

```
bottles-cli [--fvs2d <path>] [--component-catalog <url>] [--dependency-catalog <url>] <command>
```

## Commands

- `addons` — manage the addon/runner catalogs:
  - `addons refresh` — refresh the component/dependency catalogs.
  - `addons runners <list|download|delete>` — list, download, or delete runners (Wine/Proton builds).
  - `addons addons <list|download|delete>` — list, download, or delete dependency addons.
- `bottle create` — create a new bottle (name, storage backend, runner).
- `bottle list` — list existing bottles.
- `bottle manage <bottle> <subcommand>` — operate on a bottle:
  - `show` / `delete` / `stop` / `processes`
  - `install runner|addon` / `uninstall addon`
  - `program add|launch|kill`
  - `env list|set|unset`
  - `dll-overrides list|set|unset`
  - `snapshot create|list|restore`
  - `wrappers gamescope <show|enable|disable|configure>` / `wrappers mangohud <show|enable|disable>`
