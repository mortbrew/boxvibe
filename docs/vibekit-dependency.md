# Switching vibekit to a git dependency

For now boxvibe uses vibekit from `../vibekit` (a path dependency). When
`mortbrew/vibekit` exists on GitHub and `v0.1.0` is tagged, swap the line in
`Cargo.toml`:

```toml
vibekit = { git = "https://github.com/mortbrew/vibekit", tag = "v0.1.0" }
```

Then run `cargo update -p vibekit` and the four checks. To work on both at
once afterwards, use a local override instead of editing the line:

```toml
# .cargo/config.toml (not committed)
[patch."https://github.com/mortbrew/vibekit"]
vibekit = { path = "../vibekit" }
```
