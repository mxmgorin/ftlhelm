"""Unpacks the game's data folder from a GOG Linux installer into a directory.

    unpack_gog.py INSTALLER DEST

The installer is a shell script with a zip appended, which zipfile reads as it is. Each file
lands under a temporary name first, so an interrupted run leaves no truncated game file.
"""
import pathlib
import shutil
import sys
import zipfile

PREFIX = "data/noarch/game/data/"
REQUIRED = {"FTL.amd64", "ftl.dat"}
# The 32-bit binary, which the port does not run.
SKIPPED = {"FTL.x86"}
PART_EXT = ".part"


def main():
    installer, dest = (pathlib.Path(a) for a in sys.argv[1:3])
    try:
        archive = zipfile.ZipFile(installer)
    except zipfile.BadZipFile:
        sys.exit(f"{installer.name}: not a GOG installer")
    with archive:
        members = {i.filename[len(PREFIX):]: i for i in archive.infolist()
                   if i.filename.startswith(PREFIX) and not i.is_dir()}
        missing = REQUIRED - members.keys()
        if missing:
            sys.exit(f"{installer.name}: no {', '.join(sorted(missing))}")
        for rel, info in members.items():
            if rel in SKIPPED or ".." in pathlib.PurePosixPath(rel).parts:
                continue
            target = dest / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            part = target.with_name(target.name + PART_EXT)
            with archive.open(info) as src, open(part, "wb") as out:
                shutil.copyfileobj(src, out)
            part.replace(target)
            print(f"unpacked {rel}")


if __name__ == "__main__":
    main()
