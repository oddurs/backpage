# Working in this repository

Instructions for any agent making changes here. Read them before touching a file.

## Never

- **Never attribute work to an AI, an assistant, or a model.** Not in commit
  messages, trailers, PR bodies, code comments, docs, the changelog, or release
  notes. No co-author trailers, no "generated with" footers, no robot emoji.
  Everything is published under the maintainer's name. `.githooks/commit-msg`
  rejects commits that violate this, and `scripts/agent pr` refuses to open a PR
  from a branch whose history contains it.
- **Never commit to `main`.** It only advances through a merged pull request.
- **Never use `--no-verify`.** If a hook is wrong, fix the hook in its own PR.
- **Never call `cargo` from CI, hooks, or scripts.** Go through `scripts/task`.

## The one command

```sh
scripts/task check     # fmt:check, lint, test, build
```

This is the entire contract between the project and its automation. CI runs
exactly this, and so do the hooks. If you add tooling, add it to `scripts/task`
— never to a workflow file — or local runs and CI will drift.

Targets: `fmt`, `fmt:check`, `lint`, `test`, `build`, `check`.

## The loop

```sh
scripts/agent start <type>/<slug>   # branch + worktree from origin/main
cd ../.worktrees/backpage/<type>/<slug>
# ... change code, add tests ...
scripts/agent check
scripts/agent commit "<conventional commit>"
scripts/agent pr
# ... after the PR merges ...
scripts/agent done
```

One unit of work, one worktree, one branch, one PR. Never work in the primary
checkout, and never share a worktree with another agent — that is what makes
parallel work safe here.

`scripts/agent doctor` diagnoses a broken environment. `scripts/agent list`
shows what is in flight.

Branch names: `<type>/<slug>` where type is `feat`, `fix`, `chore`, `docs`,
`perf`, `refactor`, or `test`.

## Commits

Conventional Commits, imperative, subject ≤ 72 characters, no trailing period.
The body explains *why*; the diff already says what.

```
fix(render): pad rows shorter than the frame

Padding was computed from the untruncated string, so a narrow frame left the
right border ragged.
```

## Code

- Rust 1.98, edition 2024, pinned in `rust-toolchain.toml`.
- `unsafe_code` is forbidden and clippy runs at `pedantic`. Do not add
  `#[allow(...)]` to silence a lint — fix the cause, or explain the exception in
  the PR description.
- Every `pub` item needs a doc comment (`missing_docs` is a warning, and
  warnings are denied).
- No dependency is added without a reason stated in the PR. The crate currently
  has zero, and that is a feature.
- Shell scripts are POSIX `sh`, pass `shellcheck -s sh`, and use tabs.

## Scope

`backpage` renders a **read-only** dashboard. Anything that accepts input,
steals focus, or makes the desktop interactive is out of scope. Say so and stop
rather than implementing it.

Do not describe unimplemented features in `README.md` as though they work — the
"Status" section states what exists today, and the roadmap is clearly separate.
Keep it that way.
