# Working in this repository

Notes for anyone, person or agent, who builds and tests Voxear from a checkout. They are short on purpose: the
architecture is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), the plan in [docs/ROADMAP.md](docs/ROADMAP.md).

## Building

- **A build folder of your own.** In a worktree, set `CARGO_TARGET_DIR` to a folder of your own under the main
  checkout's `target/` (for example `target/my-task-build`). Never build a worktree into the shared `target/`:
  two checkouts in one folder rebuild each other's work every time.
- **Check free space before a large build**, and delete your build folder when your work has landed. A full
  drive fails at the link step with errors that do not mention space.
- Development and test builds optimise the engine's own crates a little (`opt-level = 1`) and keep line numbers
  only. Leave that as it is: unoptimised, the terrain crate's tests take nine times as long to run.

## Testing

Run the tests for what you changed, not everything. The whole suite takes over half an hour and is run now and
then, not for each change.

- **Start from the helper.** `tools/test-changed.py` prints the commands for what differs from `master`;
  `--run` runs them. It picks a changed test file whole, a changed source file's own unit tests, and the test
  files named after it in its crate and the crates that use it. Add what you know your change reaches and drop
  what it cannot: the helper goes by names.
- **Name test targets.** `cargo test -p voxear_engine --test blocks`, never `--tests` or a bare
  `cargo test -p voxear_engine` or `-p voxear_editor`: each of their hundred-odd test files links the whole
  engine into a program of its own, and building them all takes tens of gigabytes.
- **Tests that open a GPU** (the renderer's, most of the editor's, some of the engine's) run with
  `-- --test-threads=8`, or `2` while an editor, a game or another test run is using the same GPU. More at once
  runs the GPU out of memory, which shows as tests failing at random. The helper adds the flag.
- **Timing a pass on the GPU:** take the least of about sixty frames, not one frame.
- **A picture is checked close up.** Crop and enlarge the part in question and compare it with the picture
  from before; a thumbnail hides what is wrong.
- **Tests that copy `samples/Hello`** need it as committed. If your checkout has local edits there, run them in
  a clean worktree.
- Say in your report which tests you ran and which you did not, and quote a failure as it was printed.

## Conventions

- **Comments, docs, the manual and commit messages** are written as a person would write them and name the
  thing they describe. They do not name other games, engines, tools or their authors.
- **Commit messages are short**: what changed, in a line or two.
- **Formatting:** format the files you add or change, not a whole crate.
- **Every system has an easy way in and a full one.** References between assets and Objs are typed (a picker,
  an enum), never a name in a string.
- **C#** uses file-scoped namespaces and the current language.
- **Do not use `git stash`** in a checkout that others share; it sweeps up their uncommitted work.
- **Before a push:** `docs/ROADMAP.md`, the roadmap pictures (`python3 docs/images/draw_roadmap.py docs/images`)
  and the README's description of them are brought up to date with what the push adds.
