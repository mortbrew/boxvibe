# The vibekit dependency

boxvibe uses vibekit (dialogs, toasts, settings rows, input, private writes,
desktop entries) from GitHub, pinned to a tag:

```toml
vibekit = { git = "https://github.com/mortbrew/vibekit", tag = "v0.3.0" }
```

`Cargo.lock` pins the tag's commit, so a build is reproducible without a
network round trip to resolve the tag again.

## Working on both at once

To change vibekit and boxvibe together, point Cargo at a local checkout with an
override instead of editing `Cargo.toml` (the override file is not committed):

```toml
# .cargo/config.toml (not committed)
[patch."https://github.com/mortbrew/vibekit"]
vibekit = { path = "../vibekit" }
```

## Moving to a new vibekit tag

1. Push and tag the new vibekit release.
2. Change the `tag` in `Cargo.toml`.
3. Run `cargo update -p vibekit` so `Cargo.lock` pins the new commit.
4. Run the four checks (`fmt`, `test`, `clippy`, `build`).
