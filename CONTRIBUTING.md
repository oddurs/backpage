# Contributing

Thanks for taking the time. This project keeps its workflow small and
enforced — the rules below are checked by hooks and by CI, not by memory.

## Setup

```sh
git clone https://github.com/oddurs/backpage.git
cd backpage
scripts/setup          # points git at .githooks and fetches dependencies
scripts/agent doctor   # reports anything still missing
```

## The workflow

`main` only ever advances through a merged pull request. A direct push is
refused locally by `.githooks/pre-push` and on the server by branch protection.

Each unit of work gets its own branch in its own worktree, so two people — or
two agents — never share a checkout:

```
backpage/                          the primary checkout, always on main
../.worktrees/backpage/<branch>/   one directory per branch
```

```sh
scripts/agent start fix/panel-padding    # branch + worktree, from origin/main
cd ../.worktrees/backpage/fix/panel-padding

# ... make the change ...

scripts/agent check                       # same checks CI runs
scripts/agent commit "fix(render): pad rows shorter than the frame"
scripts/agent pr                          # checks, pushes, opens the PR
scripts/agent done                        # after merge: cleans up the worktree
```

`scripts/agent list` shows every worktree and its pull request.

## Branch names

`<type>/<slug>`, where type is one of `feat`, `fix`, `chore`, `docs`, `perf`,
`refactor`, `test`. The slug is lowercase and hyphenated.

## Commits

[Conventional Commits](https://www.conventionalcommits.org), imperative mood,
subject at most 72 characters, no trailing period:

```
fix(render): pad rows shorter than the frame

Rows narrower than the frame left the right border ragged, because padding
was computed from the untruncated string.
```

The body explains *why* — the diff already says what.

## Checks

```sh
scripts/task check     # fmt:check, lint, test, build
```

This is the only command CI runs, so a green local run means a green CI run.
Everything goes through `scripts/task`; nothing calls `cargo` directly.

Do not use `--no-verify`. If a hook is wrong, fix the hook.

## Pull requests

State the problem, the approach, and anything a reviewer should look at
sceptically. If a change has a known weakness, say so in the PR rather than
letting a reviewer discover it.

Approval is not required to merge — this is currently a single-maintainer
project, and requiring a review would deadlock it. Every other protection
(passing CI, up-to-date branch, resolved conversations, no force pushes) does
apply. That will change to a required review as soon as there is a second
maintainer.

Merges are squash-only, and the branch is deleted automatically.

## Scope

`backpage` is a read-only dashboard. Anything that accepts input, steals
focus, or makes the desktop interactive is out of scope by design.
