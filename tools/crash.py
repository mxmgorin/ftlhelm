#!/usr/bin/env python3
"""Prints the crash report lines of an ftlhelm log, each rip resolved with addr2line to its
function, and to its source line where the binary keeps line tables: a game address against
the game's executable, an ftlhelm+OFFSET against the ftlhelm.so that wrote the log.

    tools/crash.py ftlhelm.log --game "/path/to/FTL Faster Than Light/data/FTL.amd64" --lib ftlhelm.so
"""

import argparse
import re
import subprocess
import sys

REPORT = re.compile(r"(crash:|panic at) ")
GAME_RIP = re.compile(r"crash: rip (0x[0-9a-f]+) game$")
LIB_RIP = re.compile(r"crash: rip 0x[0-9a-f]+ ftlhelm\+(0x[0-9a-f]+)$")


def resolve(binary, address):
    """Returns addr2line's function and place for an address in binary, on one line."""
    if not binary:
        return "(no binary given)"
    out = subprocess.run(
        ["addr2line", "-f", "-C", "-e", binary, address],
        capture_output=True, text=True, check=False,
    )
    return " at ".join(out.stdout.split()) if out.returncode == 0 else out.stderr.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("log", help="ftlhelm.log, or a log holding its lines")
    parser.add_argument("--game", help="FTL.amd64 of the build that crashed")
    parser.add_argument("--lib", help="ftlhelm.so of the run that crashed")
    args = parser.parse_args()
    found = False
    with open(args.log, errors="replace") as log:
        for line in log:
            line = line.rstrip("\n")
            if not REPORT.search(line):
                continue
            found = True
            game, lib = GAME_RIP.search(line), LIB_RIP.search(line)
            if game:
                line += "  -> " + resolve(args.game, game.group(1))
            elif lib:
                line += "  -> " + resolve(args.lib, lib.group(1))
            print(line)
    if not found:
        sys.exit("no crash report in " + args.log)


if __name__ == "__main__":
    main()
