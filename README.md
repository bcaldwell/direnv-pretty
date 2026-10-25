# direnv-pretty

A thin wrapper around [direnv](https://direnv.net) that replaces its noisy output with a single status line:

```
activated direnv (flake) (1.20s)
deactivated direnv
failed activating direnv
```

When a `.envrc` takes longer than 250ms, a "loading environment" spinner is shown. The features listed come from the `use ...` lines in `.envrc`.

Tested with direnv 2.37.1 and 2.38.2.

## Install

Download the binary for your platform from the [releases page](https://github.com/bcaldwell/direnv-pretty/releases) (`x86_64-linux`, `x86_64-darwin`, `aarch64-darwin`), make it executable and put it on your `PATH`, or build from source:

```sh
cargo build --release   # target/release/direnv-pretty
```

## Usage

Replace your direnv hook with the direnv-pretty one. For zsh, in `~/.zshrc`:

```sh
eval "$(direnv-pretty hook zsh)"
```

For bash, use `hook bash`. The hook output from direnv is rewritten so that `direnv export` is run through direnv-pretty, which is found by running `direnv` from your `PATH`.

To use a specific direnv binary, pass `--direnv`:

```sh
eval "$(direnv-pretty --direnv /path/to/direnv hook zsh)"
```

Any other command (`direnv-pretty allow`, `direnv-pretty status`, ...) is passed straight through to direnv.

## Environment variables

| Variable | Effect |
| --- | --- |
| `PRETTY_DIRENV_DEBUG=true` | Always print direnv's own output, not just on errors |
| `PRETTY_DIRENV_SILENT=true` | Don't print the status line or spinner |

On errors (non-zero exit or a `direnv: error` line), direnv's full output is always shown.

## Development

The flake provides a dev shell with cargo, clippy and rustfmt (`nix develop`, or use direnv).

Releases are built by GitHub Actions when a `v*` tag is pushed.

## License

See [LICENSE](LICENSE).
