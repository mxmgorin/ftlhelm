#!/usr/bin/env python3
"""Builds the FTL port for PortMaster out of packaging/portmaster/ and the library.

The catalogue files move into the game directory and the store README is renamed after
the port, as tools/build_release.py in the PortMaster repository does. Game data stays out;
box64 comes from outside the repository. The licences README names the revision the zip is
built from as the library's source.

    tools/mkzip.py --lib ftlhelm.so --box64 PATH out.zip
"""
import argparse
import pathlib
import subprocess
import sys
import zipfile

GAME_DIR = "ftl"
LAUNCHER = "FTL.sh"
RELOCATED = {
    "port.json": f"{GAME_DIR}/port.json",
    "gameinfo.xml": f"{GAME_DIR}/gameinfo.xml",
    "screenshot.png": f"{GAME_DIR}/screenshot.png",
    "cover.png": f"{GAME_DIR}/cover.png",
    "README.md": f"{GAME_DIR}/{GAME_DIR}.md",
}
SKIPPED_DIRS = {"__pycache__"}
BOX64 = f"{GAME_DIR}/box64"
LIBRARY = f"{GAME_DIR}/ftlhelm.so"
LAYOUT = f"{GAME_DIR}/ftlhelm.ini"
LICENSES_README = f"{GAME_DIR}/licenses/README.md"
REPO_URL = "https://github.com/mxmgorin/ftlhelm"
SOURCE_LINE = "The source of `ftlhelm.so` is the repository above"
EXECUTABLES = {LAUNCHER, BOX64}
# From the repository root into the game directory: path there, name here.
REPO_FILES = {
    "config/ftlhelm.ini": "ftlhelm.ini",
    "LICENSE": "licenses/LICENSE-ftlhelm.txt",
    "NOTICE": "licenses/NOTICE-ftlhelm.txt",
    "TRADEMARKS.md": "licenses/TRADEMARKS-ftlhelm.md",
}
# The port's settings over ftlhelm's defaults: spruceOS gives no SDL mapping for the
# Smart Pro S, whose raw A and B are swapped.
PORT_SETTINGS = ("swap_buttons = 0", "swap_buttons = 1")


def port_layout(text):
    """Applies the port's settings to ftlhelm.ini; returns the new text."""
    old, new = PORT_SETTINGS
    if old not in text:
        sys.exit(f"ftlhelm.ini: no '{old}' line")
    return text.replace(old, new)


def revision(repo):
    """Names the checked-out revision: its tag if it has one, else its commit; returns the name."""
    for args in (["describe", "--tags", "--exact-match"], ["rev-parse", "--short=12", "HEAD"]):
        out = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
        if out.returncode == 0:
            return out.stdout.strip()
    sys.exit("git: no revision to name as the library's source")


def pin_source(text, rev):
    """Points the library's source line at one revision; returns the new text."""
    if SOURCE_LINE not in text:
        sys.exit(f"licenses README: no '{SOURCE_LINE}'")
    pinned = f"The source of `ftlhelm.so` is [{rev}]({REPO_URL}/tree/{rev}) in the repository above"
    return text.replace(SOURCE_LINE, pinned)


def entries(repo, lib, box64):
    """Lists the files that ship; returns [(path, name in the zip)]."""
    port = repo / "packaging" / "portmaster"
    found = [(port / LAUNCHER, LAUNCHER)]
    found += [(port / name, arcname) for name, arcname in sorted(RELOCATED.items())]
    for path in sorted((port / GAME_DIR).rglob("*")):
        rel = path.relative_to(port)
        if path.is_file() and not SKIPPED_DIRS & set(rel.parts):
            found.append((path, rel.as_posix()))
    found += [(repo / src, f"{GAME_DIR}/{name}") for src, name in REPO_FILES.items()]
    found += [(lib, LIBRARY), (box64, BOX64)]
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--lib", type=pathlib.Path, required=True, help="ftlhelm.so to pack")
    parser.add_argument("--box64", type=pathlib.Path, required=True, help="aarch64 box64 to pack")
    parser.add_argument("out", type=pathlib.Path, help="zip to write")
    args = parser.parse_args()

    repo = pathlib.Path(__file__).resolve().parents[1]
    files = entries(repo, args.lib, args.box64)
    missing = [str(p) for p, _ in files if not p.is_file()]
    if missing:
        sys.exit("missing: " + ", ".join(missing))
    rev = revision(repo)
    status = subprocess.run(["git", "-C", str(repo), "status", "--porcelain"], capture_output=True, text=True)
    if status.stdout:
        print(f"warning: uncommitted changes, the source line names {rev}", file=sys.stderr)
    args.out.parent.mkdir(parents=True, exist_ok=True)

    # Explicit directory entries: not every extractor creates missing parents.
    dirs = sorted({f"{p}/" for _, a in files for p in pathlib.PurePosixPath(a).parents if str(p) != "."})
    with zipfile.ZipFile(args.out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for d in dirs:
            info = zipfile.ZipInfo(d)
            info.external_attr = (0o40755 << 16) | 0x10
            z.writestr(info, b"")
        for path, arcname in files:
            info = zipfile.ZipInfo.from_file(path, arcname)
            # The mode is set here: a launcher unpacked without +x does not run.
            info.external_attr = (0o755 if arcname in EXECUTABLES else 0o644) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            data = path.read_bytes()
            if arcname == LAYOUT:
                data = port_layout(data.decode()).encode()
            elif arcname == LICENSES_README:
                data = pin_source(data.decode(), rev).encode()
            z.writestr(info, data, compresslevel=9)
    print(f"{len(files)} files, {args.out.stat().st_size:,} B in {args.out}")


if __name__ == "__main__":
    main()
