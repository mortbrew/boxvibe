# The vibekit dependency

boxvibe 0.5.0 shipped with vibekit `v0.1.0` from GitHub. The `vibekit-dialogs`
branch needs vibekit 0.2.0 (dialogs, toasts, settings rows), which is only on
vibekit's local `dialogs` branch, so for now `Cargo.toml` uses the path:

```toml
vibekit = { path = "../vibekit" }
```

**Once vibekit `v0.2.0` is pushed and tagged**, replace that line with:

```toml
vibekit = { git = "https://github.com/mortbrew/vibekit", tag = "v0.2.0" }
```

Then run `cargo update -p vibekit` (Cargo.lock then pins the tag's commit) and
the four checks. To work on both at once afterwards, use a local override
instead of editing the line:

```toml
# .cargo/config.toml (not committed)
[patch."https://github.com/mortbrew/vibekit"]
vibekit = { path = "../vibekit" }
```
