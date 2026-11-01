"""Fork-only comparison: same machine, warmed dependencies, no cross-run cache."""
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

ROOT = Path.cwd()
REPORT = ROOT / "layout-cost.json"
RESULT = {"platform": platform.platform(), "cpu_count": os.cpu_count(), "revisions": {}}
BASE = "bb03a6b2e"
REVISED = "42a5981e94b51e4ed95411af91c63e0f6a7cb192"


def run(label, command):
    started = time.monotonic()
    with (ROOT / f"layout-{label}.log").open("w") as log:
        subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)
    seconds = round(time.monotonic() - started, 3)
    print(label, seconds, flush=True)
    return seconds


def save():
    REPORT.write_text(json.dumps(RESULT, indent=2) + "\n")


RESULT["rustc"] = subprocess.check_output(["rustc", "-Vv"], text=True)
for name, revision in [("baseline", BASE), ("revised", REVISED)]:
    subprocess.run(["git", "checkout", "--detach", revision], check=True)
    packages = ["-p", "horizon-client"]
    core_package = "horizon-client"
    if name == "revised":
        packages += ["-p", "horizon-client-lib"]
        core_package = "horizon-client-lib"
    tests = ["cargo", "test", "--locked", *packages, "--tests", "--no-run"]
    # Warm dependencies first; clean only the client packages before each timed build.
    run(f"{name}-warm-tests", tests)
    row = RESULT["revisions"][name] = {"revision": revision, "test_command": tests, "client_test_seconds": []}
    for sample in range(2):
        run(f"{name}-clean-{sample}", ["cargo", "clean", *packages])
        row["client_test_seconds"].append(run(f"{name}-tests-{sample}", tests))
        save()
    probe = ["cargo", "build", "--locked", "-p", core_package,
             "--example", "client_probe", "--bin", "horizon-ctl"]
    run(f"{name}-warm-tools", probe)
    dep_info = Path(os.environ["CARGO_TARGET_DIR"]) / "debug/examples/client_probe.d"
    dependencies = dep_info.read_text()
    assert "/src/game/" not in dependencies and "/src/game.rs" not in dependencies
    row["gameplay_sources_in_probe_depinfo"] = 0
    row["tools_after_game_edit_seconds"] = []
    source = ROOT / "crates/horizon-client/src/game/combat.rs"
    original = source.read_bytes()
    try:
        for sample in range(2):
            source.write_bytes(original + f"\n// benchmark invalidation {sample}\n".encode())
            row["tools_after_game_edit_seconds"].append(run(f"{name}-tools-{sample}", probe))
            save()
    finally:
        source.write_bytes(original)
    row["median_client_test_seconds"] = statistics.median(row["client_test_seconds"])
    row["median_tools_after_game_edit_seconds"] = statistics.median(row["tools_after_game_edit_seconds"])
    save()
print(json.dumps(RESULT, indent=2))
