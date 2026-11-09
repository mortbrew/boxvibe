# Repository setup

This file is the **specification** for how a mortbrew "vibe" repository is set
up. It is not a description of what happened once — it is the checklist that
every repo must satisfy, and the procedure for bringing a new one up to it.

If you are an agent asked to "set up this repo like boxvibe", or to "use
setup.md", follow this file top to bottom and then run the **Verification**
section. Do not skip a step because the repo "looks fine"; every item here
exists because something went wrong without it.

boxvibe is the reference implementation. When this file and a repo disagree,
this file wins and the repo is the thing to fix.

---

## 1. The branch model

Two long-lived branches. Nothing else.

| Branch | Role |
| --- | --- |
| `dev` | **The default branch.** The only branch worked on. Every feature PR targets it. |
| `main` | **A release marker.** Accepts changes only through a pull request. `dev` is merged into it only to cut a release. |

Rules that follow from this:

- `dev` is the repository's default branch, so clones, PRs and the web UI all
  land there by default.
- `main` is protected by the "Protect main" ruleset (§4): no force-push, no
  deletion, no direct push, and `fmt`, `ci` and `msrv` must be green.
- Feature branches are short-lived and deleted after merge. If a branch's work
  is merged, the branch goes.
- Never leave a stale branch on origin. A branch that is not `dev` or `main`
  and has no open PR is either about to be merged or should be deleted.

## 2. Files that must exist

All of these are required. Copy them from boxvibe rather than retyping.

```
.github/workflows/ci.yml
.github/workflows/audit.yml
.github/workflows/codeql.yml
.github/workflows/release.yml
.github/dependabot.yml
.cursor/rules/versioning-and-testing.mdc
docs/setup.md          <- this file
```

### `.github/workflows/ci.yml`

Three jobs, so a formatting failure answers in seconds instead of after a full
build. `msrv` reads `rust-version` from `Cargo.toml` so the README's stated
minimum cannot drift away from what actually builds.

```yaml
# Build, test and lint on Linux for every push and pull request to main or dev.
name: ci

on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main, dev]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

# A newer push to the same branch makes the run in flight pointless.
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  # Formatting needs no build, so it answers in seconds.
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --check

  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release --locked
      - run: cargo test --locked
      - run: cargo clippy --locked --all-targets -- -D warnings

  # The README promises a minimum Rust version, so build on exactly that. The
  # version is read from Cargo.toml's `rust-version` so the two cannot drift.
  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - name: Read the minimum version from Cargo.toml
        id: msrv
        run: echo "version=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)" >> "$GITHUB_OUTPUT"
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ steps.msrv.outputs.version }}
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --locked
```

If the crate needs system libraries to build (ALSA, pkg-config, OpenSSL), add
the `apt-get install` line to **both** the `ci` and `msrv` jobs. A job that
cannot build is a job that silently never runs.

### `.github/workflows/audit.yml`

```yaml
# `cargo audit`: fails when a dependency in Cargo.lock has a known security
# advisory (RustSec). Runs on main, dev, pull requests and weekly.
name: audit

on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main, dev]
  schedule:
    - cron: "37 4 * * 1"

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-audit --locked
      - run: cargo audit
```

### `.github/workflows/codeql.yml`

```yaml
# CodeQL code scanning for Rust (no build needed) on main, dev, pull requests and weekly.
name: codeql

on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main, dev]
  schedule:
    - cron: "17 4 * * 1"

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  analyze:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write
    steps:
      - uses: actions/checkout@v7
      - uses: github/codeql-action/init@v4
        with:
          languages: rust
          build-mode: none
      - uses: github/codeql-action/analyze@v4
        with:
          category: "/language:rust"
```

### `.github/workflows/release.yml`

On every `v*` tag: build the Linux archives and attach them to the GitHub
release. File names carry no version, so links can point at
`https://github.com/mortbrew/<repo>/releases/latest/download/<file>`.

```yaml
# On every `v*` tag: build Linux (x86_64, aarch64) and attach the archives to
# the GitHub release. File names carry no version, so links can point at
# https://github.com/mortbrew/<repo>/releases/latest/download/<file>.
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:
    inputs:
      tag:
        description: "Existing tag to (re)build, e.g. v0.1.0"
        required: true

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always
  TAG: ${{ inputs.tag || github.ref_name }}

jobs:
  create:
    runs-on: ubuntu-latest
    steps:
      - name: Release (once; the builds below add their files to it)
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release view "$TAG" -R "$GITHUB_REPOSITORY" >/dev/null 2>&1 || gh release create "$TAG" -R "$GITHUB_REPOSITORY" --verify-tag --generate-notes --title "<repo> $TAG"

  linux:
    needs: create
    strategy:
      fail-fast: false
      matrix:
        include:
          # The oldest supported runners: the binary then runs on older glibc too.
          - arch: x86_64
            runner: ubuntu-22.04
          # GitHub's arm64 runners (free for public repositories).
          - arch: aarch64
            runner: ubuntu-22.04-arm
    runs-on: ${{ matrix.runner }}
    steps:
      - uses: actions/checkout@v7
        with:
          ref: ${{ env.TAG }}
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --locked
      - run: cargo build --release --locked
      - name: Package
        run: |
          mkdir -p dist/<repo>
          cp target/release/<repo> README.md dist/<repo>/
          tar -C dist -czf dist/<repo>-${{ matrix.arch }}-linux.tar.gz <repo>
      - uses: softprops/action-gh-release@v3
        with:
          tag_name: ${{ env.TAG }}
          fail_on_unmatched_files: true
          files: dist/<repo>-${{ matrix.arch }}-linux.tar.gz
```

Replace `<repo>` with the crate name.

### `.github/dependabot.yml`

Dependabot PRs target `dev`, never `main`.

```yaml
# Weekly pull requests for newer crates and newer GitHub Actions.
version: 2
updates:
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly
    target-branch: "dev"
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
    target-branch: "dev"
```

### `.cursor/rules/versioning-and-testing.mdc`

The agent-facing framing of this file: never test outside the repo, keep builds
under `target/`, don't "fix" the version, read every check before merging, and
ask before anything outward-facing. Copy boxvibe's and change the crate name.

## 3. Repository settings

Set these with `gh api -X PATCH repos/mortbrew/<repo>`. The values are not
preferences; they are what makes the branch model hold.

| Setting | Value | Why |
| --- | --- | --- |
| `default_branch` | `dev` | Clones and PRs land on the branch that is worked on. |
| `has_issues` | `true` | Somewhere to report bugs that is not a PR. |
| `has_wiki` | `true` | Matches the other repos. |
| `has_projects` | `true` | Matches the other repos. |
| `has_discussions` | `false` | Not used. |
| `allow_squash_merge` | `true` | |
| `allow_merge_commit` | `true` | |
| `allow_rebase_merge` | `true` | |
| `allow_auto_merge` | `false` | Merges are deliberate. |
| `allow_update_branch` | `false` | |
| `delete_branch_on_merge` | `false` | Branches are deleted deliberately, see §6. |
| `homepage` | `""` | Never point at an upstream project's site. |
| `description` | one line, what it is | |
| `topics` | as appropriate | |

```sh
gh api -X PATCH repos/mortbrew/<repo> \
  -f default_branch=dev \
  -F has_issues=true -F has_wiki=true -F has_projects=true -F has_discussions=false \
  -F allow_squash_merge=true -F allow_merge_commit=true -F allow_rebase_merge=true \
  -F allow_auto_merge=false -F allow_update_branch=false -F delete_branch_on_merge=false \
  -f homepage=''
```

## 4. The "Protect main" ruleset

This is the safety net. It is a **ruleset**, not legacy branch protection, and
it names `refs/heads/main` **explicitly** — never `~DEFAULT_BRANCH`. The
default branch is `dev`, so a `~DEFAULT_BRANCH` ruleset would protect the wrong
branch the moment the default changes. That mistake has already happened once.

Create it with:

```sh
gh api -X POST repos/mortbrew/<repo>/rulesets --input - <<'JSON'
{
  "name": "Protect main",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": { "include": ["refs/heads/main"], "exclude": [] }
  },
  "rules": [
    { "type": "non_fast_forward" },
    { "type": "deletion" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": false,
        "required_reviewers": [],
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false,
        "require_extra_approval_for_unattributed_changes": true,
        "allowed_merge_methods": ["merge", "squash", "rebase"]
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "do_not_enforce_on_create": false,
        "required_status_checks": [
          { "context": "fmt" },
          { "context": "ci" },
          { "context": "msrv" }
        ]
      }
    }
  ]
}
JSON
```

To change an existing ruleset, `PUT` the same body to
`repos/mortbrew/<repo>/rulesets/<id>`.

What each rule buys:

- `non_fast_forward` — no force-push over published history.
- `deletion` — `main` cannot be deleted.
- `pull_request` — no direct push to `main`; it only moves by PR.
- `required_status_checks` — `fmt`, `ci` and `msrv` must pass first.

**The required list is deliberately short.** `audit` and `codeql` are not
required, because they are slow and occasionally flaky, and a required check
that flakes blocks releases. That is exactly why §7 exists: the required three
are the *minimum*, not the whole story.

## 5. Security settings

All of these must be **on**:

| Setting | How |
| --- | --- |
| Secret scanning | on by default for public repos; verify |
| Secret scanning push protection | on |
| Dependabot alerts | `gh api -X PUT repos/mortbrew/<repo>/vulnerability-alerts` |
| Dependabot security updates | `gh api -X PUT repos/mortbrew/<repo>/automated-security-fixes` |
| CodeQL | the `codeql.yml` workflow in §2 |

```sh
gh api -X PUT repos/mortbrew/<repo>/vulnerability-alerts
gh api -X PUT repos/mortbrew/<repo>/automated-security-fixes
```

Verify with:

```sh
gh api repos/mortbrew/<repo> -q '.security_and_analysis'
```

`dependabot_security_updates` must read `enabled`, and `secret_scanning` and
`secret_scanning_push_protection` must both read `enabled`.

## 6. Branch hygiene

- Only `dev` and `main` live on origin between releases.
- A merged feature branch is deleted, locally and on origin.
- Before deleting a branch, record its SHA somewhere recoverable. Deleting a
  branch is a history rewrite from the reader's point of view, so it needs
  Toby's approval first (§8).
- After deleting branches, prune the local remote-tracking refs and remove the
  stale `branch.*` sections from `.git/config`:

```sh
git fetch origin --prune
git remote prune origin
for b in <deleted branches>; do git config --remove-section "branch.$b"; done
```

## 7. Before merging or releasing, read every check

The ruleset only *requires* `fmt`, `ci` and `msrv`. A pull request can therefore
merge with `audit` or `codeql` red. That has happened: a release PR merged while
CodeQL was failing, because only the required three were looked at.

- Run `gh pr checks <n>` and read **all** of it, not just the required contexts.
  Anything not `pass` is a blocker until it is understood.
- `codeql` failing means new code-scanning alerts. Read them with
  `gh api repos/mortbrew/<repo>/code-scanning/alerts?state=open` and fix or
  dismiss each one with a reason. Do not merge on top of them.
- `audit` failing means a RustSec advisory in `Cargo.lock`. Bump the dependency;
  do not ignore it.

## 8. Ask before anything outward-facing

Anything a third party can see needs Toby's approval **before** it happens, not
after. Show the exact text or the exact command and wait.

- Public comments on issues and pull requests, and closing, reopening, labeling
  or assigning them. "Can we post a response to that issue?" is permission to
  *draft* one, not to publish it.
- Releases, tags, and pushes to `main`.
- Force-pushes, branch deletion, and anything that rewrites published history.

Local work — editing files, running tests, committing and pushing to `dev` —
does not need this approval.

## 9. Versions

`dev`'s `Cargo.toml` version runs **ahead** of the newest published release.
That is deliberate: the version string is drawn on screen, so seeing the higher
number confirms the running binary is a build from source rather than the
released one.

- Keep the bump in `Cargo.toml` only; `env!("CARGO_PKG_VERSION")` propagates it.
- Update `Cargo.lock` with `cargo update --workspace --offline`, never by hand.
- `CHANGELOG.md` sections are historical records of shipped releases, so they
  keep the version that shipped.
- After a release, bump `dev` ahead of the tag that was just cut.
- A source build reporting no update available is expected, not a bug.

## 10. Verification

Run all of this. Every line must pass before the repo counts as set up.

```sh
# 1. Exactly two branches, and dev is the default.
gh api repos/mortbrew/<repo>/branches --paginate -q '.[].name'   # dev, main
gh api repos/mortbrew/<repo> -q '.default_branch'                # dev

# 2. The ruleset protects main, and requires the three checks.
gh api repos/mortbrew/<repo>/rulesets -q '.[].id' | while read id; do
  gh api repos/mortbrew/<repo>/rulesets/$id \
    -q '{name:.name, include:.conditions.ref_name.include, rules:[.rules[].type],
         required:[.rules[]|select(.type=="required_status_checks")|.parameters.required_status_checks[].context]}'
done
# expect: include ["refs/heads/main"], rules include pull_request and
#         required_status_checks, required ["fmt","ci","msrv"]

# 3. Security settings are on.
gh api repos/mortbrew/<repo> -q '.security_and_analysis'

# 4. Every workflow is valid YAML and passes actionlint.
actionlint .github/workflows/*.yml

# 5. The four checks are green on dev.
gh run list --branch dev --limit 4

# 6. No code-scanning alerts.
gh api repos/mortbrew/<repo>/code-scanning/alerts -q 'length'    # 0

# 7. The tree passes the checks locally, on the declared MSRV too.
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo +$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml) build --locked

# 8. Nothing local is unpushed.
git log --oneline --all --not --remotes=origin | wc -l           # 0
```

## 11. Bringing a new repo up to this

1. Create the repo, then set the settings in §3.
2. Push `dev` and `main`. Make `dev` the default **before** creating the
   ruleset, so the ruleset's explicit `refs/heads/main` is unambiguous.
3. Add the files in §2. Fix the crate name in `release.yml`.
4. Create the ruleset in §4.
5. Turn on the security settings in §5.
6. Push to `dev` and wait for `ci`, `audit` and `codeql` to go green. Fix
   whatever they find — a new repo almost always has formatting and clippy
   drift, and the declared `rust-version` is usually wrong.
7. Run §10. Do not call the repo set up until every line passes.
