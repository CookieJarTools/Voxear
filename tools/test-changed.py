#!/usr/bin/env python3
"""Picks the tests that cover what a change touches, and prints or runs them.

    tools/test-changed.py                 # what differs from master, with uncommitted and new files
    tools/test-changed.py --run           # the same, and runs each command in turn
    tools/test-changed.py --base origin/master
    tools/test-changed.py --range A..B    # a span of commits, working tree left out

It never builds every test of a crate: the engine's and the editor's test programs are hundreds of megabytes
each. A changed test file is run whole. A changed source file runs its own module's unit tests, and the test
files of its crate and of the crates that use it whose names share a word with its path. Shaders, the C# core,
the manual and the docs are mapped the same way.

It is a first pick, not a proof: add the tests you know your change reaches, and leave out what it cannot.
Words that should lead to test files not named after them go in ALIASES below.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A word in a changed path, and words of test files it should also pick.
ALIASES = {
    "block": ["terrain", "blocks", "map"],
    "biome": ["terrain", "blocks"],
    "generate": ["generator", "terrain", "blocks", "caves"],
    "structure": ["blocks", "terrain"],
    "stream": ["blocks", "terrain", "origin"],
    "flow": ["blocks", "water"],
    "weather": ["storms"],
    "ocean": ["sea", "water"],
    "water": ["sea"],
    "foliage": ["plants"],
    "grown": ["plants"],
    "boxmodel": ["box"],
    "inspector": ["drawers", "fields"],
    "props": ["editor"],
    "content": ["loading"],
}

# Words too common to tell anything.
DULL = {"mod", "lib", "main", "src", "tests", "test", "rs", "common", "util", "utils", "types"}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def changed_files(args):
    if args.range:
        return sorted(set(git("diff", "--name-only", args.range).split("\n")) - {""})
    base = git("merge-base", args.base, "HEAD").strip()
    files = set(git("diff", "--name-only", base, "HEAD").split("\n"))
    files |= set(git("diff", "--name-only", "HEAD").split("\n"))
    files |= set(git("ls-files", "--others", "--exclude-standard").split("\n"))
    return sorted(files - {""})


def workspace():
    """Each crate's folder name, with the crates of the workspace it uses."""
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    packages = json.loads(out)["packages"]
    names = {package["name"] for package in packages}
    return {
        package["name"]: {dep["name"] for dep in package["dependencies"] if dep["name"] in names}
        for package in packages
    }


def users_of(crate, uses):
    """`crate` and every crate that uses it, however far up."""
    found, grew = {crate}, True
    while grew:
        more = {name for name, deps in uses.items() if deps & found} - found
        found |= more
        grew = bool(more)
    return found


def words(text, aliases=False):
    """The words of a path or a name, each with its singular and plural; with `aliases`, what they lead to too."""
    out = set()
    for word in re.split(r"[^a-z0-9]+", text.lower()):
        if len(word) < 3 or word in DULL:
            continue
        out |= {word, word.rstrip("s"), word + "s"}
    for word in list(out) if aliases else []:
        for other in ALIASES.get(word, []):
            out |= {other, other.rstrip("s"), other + "s"}
    return out


def test_files(crate):
    folder = ROOT / "crates" / crate / "tests"
    return sorted(folder.glob("*.rs")) if folder.is_dir() else []


def opens_a_gpu(path):
    text = path.read_text(errors="replace")
    return "new_offscreen" in text or "Renderer::new" in text


def module_filter(below_src):
    """The unit tests of the module a source file is: `blocks/stream.rs` is `blocks::stream::`."""
    parts = list(Path(below_src).with_suffix("").parts)
    if parts and parts[-1] in ("mod", "lib", "main"):
        parts.pop()
    return "::".join(parts) + "::" if parts else None


def pick(files, uses):
    """Unit test filters by crate, test files by crate, and what could not be mapped."""
    unit, whole, notes = {}, {}, []

    def matching(crates, keys, why):
        hit = False
        for crate in sorted(crates):
            for path in test_files(crate):
                if words(path.stem) & keys:
                    whole.setdefault(crate, set()).add(path.stem)
                    hit = True
        if not hit:
            notes.append(f"no test file is named after {why}: only unit tests, if any, are picked")

    for file in files:
        parts = Path(file).parts
        if len(parts) >= 4 and parts[0] == "crates" and parts[2] == "tests" and file.endswith(".rs"):
            if len(parts) == 4:
                whole.setdefault(parts[1], set()).add(Path(file).stem)
            else:
                notes.append(f"{file} is shared by its crate's tests: run the ones that use it")
        elif len(parts) >= 4 and parts[0] == "crates" and parts[2] == "src" and file.endswith(".rs"):
            crate, below = parts[1], str(Path(*parts[3:]))
            filter_ = module_filter(below)
            unit.setdefault(crate, set()).add(filter_ or "")
            if filter_:
                matching(users_of(crate, uses), words(below, True), file)
            else:
                notes.append(f"{file} is its crate's root: every unit test of {crate} is picked")
        elif len(parts) >= 4 and parts[0] == "crates" and parts[2] == "shaders":
            matching(users_of(parts[1], uses), words(Path(file).stem, True), file)
        elif parts[0] == "managed" and file.endswith(".cs"):
            whole.setdefault("#dotnet", set()).add("managed/Voxear.Core.Tests")
            keys = {"script" + word for word in words(Path(file).stem)} | words(Path(file).stem)
            for path in test_files("voxear_engine"):
                stem = path.stem
                if stem.startswith("script") and words(stem.removeprefix("script_").removeprefix("scripts_")) & keys:
                    whole.setdefault("voxear_engine", set()).add(stem)
        elif parts[0] in ("manual", "docs") and file.endswith(".md"):
            unit.setdefault("#manual", set()).add("")
        elif parts[0] == "crates" and file.endswith(("Cargo.toml", "build.rs")):
            notes.append(f"{file} changes how its crate builds: nothing is picked for it")
        elif file in ("Cargo.toml", "Cargo.lock"):
            notes.append(f"{file} changes how everything builds: nothing is picked for it")
    return unit, whole, notes


def gpu_threads(asked):
    if asked:
        return asked
    # Others drawing on the same GPU leave room for two tests at once, not eight.
    busy = subprocess.run(["pgrep", "-fc", r"(vox|voxear-editor) (editor|run)|/debug/deps/[a-z_]+-[0-9a-f]{16}"],
                          capture_output=True, text=True).stdout.strip()
    return 2 if busy.isdigit() and int(busy) > 0 else 8


def commands(unit, whole, threads):
    out = []
    if "#manual" in unit:
        out.append(["cargo", "test", "-p", "voxear_docs", "--test", "docs", "--",
                    "the_engines_manual_has_no_broken_links_or_missing_pictures"])
    if "#dotnet" in whole:
        out.append(["dotnet", "test", "managed/Voxear.Core.Tests", "-c", "Debug", "--nologo", "-v", "q"])
    for crate in sorted(set(unit) | set(whole)):
        if crate.startswith("#"):
            continue
        filters = unit.get(crate)
        if filters is not None:
            every = "" in filters
            out.append(["cargo", "test", "-p", crate, "--lib"] + ([] if every else ["--", *sorted(filters)]))
        plain, gpu = [], []
        for stem in sorted(whole.get(crate, ())):
            path = ROOT / "crates" / crate / "tests" / f"{stem}.rs"
            (gpu if path.is_file() and opens_a_gpu(path) else plain).append(stem)
        for stems, tail in ((plain, []), (gpu, ["--", f"--test-threads={threads}"])):
            if stems:
                out.append(["cargo", "test", "-p", crate] + [x for stem in stems for x in ("--test", stem)] + tail)
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base", default="master", help="what to compare with (default: master)")
    parser.add_argument("--range", help="a span of commits, as A..B, in place of the working tree's changes")
    parser.add_argument("--run", action="store_true", help="run the commands, not only print them")
    parser.add_argument("--gpu-threads", type=int, help="tests at once in programs that open a GPU (default: 8, or 2 while others use it)")
    args = parser.parse_args()

    files = changed_files(args)
    if not files:
        print("Nothing differs.")
        return 0
    unit, whole, notes = pick(files, workspace())
    picked = commands(unit, whole, gpu_threads(args.gpu_threads))
    for note in sorted(set(notes)):
        print(f"# {note}", file=sys.stderr)
    count = sum(len(stems) for stems in whole.values())
    if count > 12:
        print(f"# {count} test files picked: a wide change. Trim to what it can reach before running.", file=sys.stderr)
    if not picked:
        print("No tests picked for these files.")
        return 0
    for command in picked:
        print(" ".join(command))
    if not args.run:
        return 0
    target = os.environ.get("CARGO_TARGET_DIR", "")
    if not target or Path(target).resolve() == (ROOT / "target").resolve():
        print("Set CARGO_TARGET_DIR to a build folder of your own before --run.", file=sys.stderr)
        return 2
    failed = []
    for command in picked:
        print(f"\n== {' '.join(command)}", flush=True)
        if subprocess.run(command, cwd=ROOT).returncode != 0:
            failed.append(" ".join(command))
    for command in failed:
        print(f"FAILED: {command}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
