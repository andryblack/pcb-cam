#!/usr/bin/env python3
"""Bump and verify PCB-CAM release versions.

Usage:
    ./scripts/version.py check [TAG]
    ./scripts/version.py bump VERSION

`check` prints the current version on stdout and verifies that Cargo.toml,
Cargo.lock, the macOS bundle metadata, README, AGENTS.md and the generated
license notices all agree. If TAG is given it must be `vVERSION`.

`bump` updates every place that carries the release version. Cargo.lock is
updated in place because a version-only change does not alter dependency
resolution; the license notices are updated in place for the workspace crates
and are still verified by the full cargo-about check in CI.
"""

import re
import sys
from pathlib import Path
from typing import Dict, Optional

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE_CRATES = ("pcb-cam", "pcb-cam-core", "pcb-cam-engine")

VERSION_PATTERN = r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?"
VERSION_RE = re.compile(rf"^{VERSION_PATTERN}$")
TAG_RE = re.compile(rf"^v({VERSION_PATTERN})$")

CARGO_TOML = ROOT / "Cargo.toml"
APP_CARGO_TOML = ROOT / "crates/pcb-cam/Cargo.toml"
CARGO_LOCK = ROOT / "Cargo.lock"
README = ROOT / "README.md"
AGENTS = ROOT / "AGENTS.md"
LICENSES = ROOT / "THIRD_PARTY_LICENSES.md"

README_STATUS_RE = re.compile(
    rf"(?m)^(\*\*Milestone [^\n]*?Version )({VERSION_PATTERN})(\.)$"
)
AGENTS_VERSION_RE = re.compile(
    rf"(The\s+current release line is )({VERSION_PATTERN})( \(Milestone )"
)
LICENSE_ENTRY_RE = re.compile(
    rf"(?m)^- (pcb-cam(?:-core|-engine)?) ({VERSION_PATTERN})$"
)


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def relative(path: Path) -> str:
    return str(path.relative_to(ROOT))


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {relative(path)}: {error}")


def write(path: Path, text: str) -> None:
    try:
        with open(path, "w", encoding="utf-8", newline="\n") as file:
            file.write(text)
    except OSError as error:
        fail(f"cannot write {relative(path)}: {error}")


def section_span(text: str, section: str, path: Path) -> tuple:
    header = re.search(rf"(?m)^\[{re.escape(section)}\][ \t]*$", text)
    if not header:
        fail(f"{relative(path)}: missing [{section}]")
    start = header.end()
    following = re.search(r"(?m)^\[", text[start:])
    end = start + following.start() if following else len(text)
    return start, end


def section_value(text: str, section: str, key: str, path: Path) -> str:
    start, end = section_span(text, section, path)
    match = re.search(
        rf'(?m)^{re.escape(key)}[ \t]*=[ \t]*"([^"]*)"', text[start:end]
    )
    if not match:
        fail(f"{relative(path)}: missing {key} in [{section}]")
    return match.group(1)


def set_section_value(text: str, section: str, key: str, value: str, path: Path) -> str:
    start, end = section_span(text, section, path)
    body, count = re.subn(
        rf'(?m)^{re.escape(key)}[ \t]*=[ \t]*"[^"]*"',
        f'{key} = "{value}"',
        text[start:end],
        count=1,
    )
    if count != 1:
        fail(f"{relative(path)}: missing {key} in [{section}]")
    return text[:start] + body + text[end:]


def lock_versions(text: str) -> Dict[str, str]:
    versions = {}
    package = None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped == "[[package]]":
            package = None
            continue
        match = re.fullmatch(r'name = "([^"]+)"', stripped)
        if match:
            package = match.group(1)
            continue
        match = re.fullmatch(r'version = "([^"]+)"', stripped)
        if match and package in WORKSPACE_CRATES:
            versions[package] = match.group(1)
    return versions


def set_lock_versions(text: str, version: str) -> str:
    lines = text.splitlines(keepends=True)
    package = None
    updated = set()
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[[package]]":
            package = None
            continue
        match = re.fullmatch(r'name = "([^"]+)"', stripped)
        if match:
            package = match.group(1)
            continue
        if package in WORKSPACE_CRATES and re.fullmatch(r'version = "[^"]+"', stripped):
            ending = line[len(line.rstrip("\r\n")) :]
            lines[index] = f'version = "{version}"{ending}'
            updated.add(package)
    missing = set(WORKSPACE_CRATES) - updated
    if missing:
        fail(f"Cargo.lock: missing package entries for {', '.join(sorted(missing))}")
    return "".join(lines)


def readme_version(text: str) -> str:
    match = README_STATUS_RE.search(text)
    if not match:
        fail(f"{relative(README)}: could not find the project status version")
    return match.group(2)


def set_readme_version(text: str, version: str) -> str:
    body, count = README_STATUS_RE.subn(
        lambda match: match.group(1) + version + match.group(3), text
    )
    if count != 1:
        fail(f"{relative(README)}: could not find the project status version")
    return body


def agents_version(text: str) -> str:
    match = AGENTS_VERSION_RE.search(text)
    if not match:
        fail(f"{relative(AGENTS)}: could not find the release line version")
    return match.group(2)


def set_agents_version(text: str, version: str) -> str:
    body, count = AGENTS_VERSION_RE.subn(
        lambda match: match.group(1) + version + match.group(3), text
    )
    if count != 1:
        fail(f"{relative(AGENTS)}: could not find the release line version")
    return body


def license_versions(text: str) -> Dict[str, str]:
    return {
        match.group(1): match.group(2) for match in LICENSE_ENTRY_RE.finditer(text)
    }


def set_license_versions(text: str, version: str) -> str:
    body, count = LICENSE_ENTRY_RE.subn(
        lambda match: f"- {match.group(1)} {version}", text
    )
    if count != len(WORKSPACE_CRATES):
        fail(
            f"{relative(LICENSES)}: expected {len(WORKSPACE_CRATES)} workspace "
            f"license entries, found {count}"
        )
    return body


def check_version(tag: Optional[str]) -> str:
    version = section_value(read(CARGO_TOML), "workspace.package", "version", CARGO_TOML)
    errors = []

    def expect(label: str, actual: Optional[str]) -> None:
        if actual != version:
            shown = "<missing>" if actual is None else actual
            errors.append(f"{label}: {shown} (expected {version})")

    expect(
        "crates/pcb-cam/Cargo.toml [package.metadata.bundle].version",
        section_value(
            read(APP_CARGO_TOML), "package.metadata.bundle", "version", APP_CARGO_TOML
        ),
    )

    locked = lock_versions(read(CARGO_LOCK))
    for crate in WORKSPACE_CRATES:
        expect(f"Cargo.lock {crate}", locked.get(crate))

    expect("README.md project status", readme_version(read(README)))
    expect("AGENTS.md release line", agents_version(read(AGENTS)))

    licensed = license_versions(read(LICENSES))
    for crate in WORKSPACE_CRATES:
        expect(f"THIRD_PARTY_LICENSES.md {crate}", licensed.get(crate))

    if tag is not None:
        match = TAG_RE.fullmatch(tag)
        if not match:
            errors.append(
                f"release tag {tag!r} must look like vMAJOR.MINOR.PATCH "
                "(optionally -suffix)"
            )
        elif match.group(1) != version:
            errors.append(f"release tag {tag} does not match version {version}")

    if errors:
        print("error: release version is inconsistent:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        print("run ./scripts/version.py bump X.Y.Z", file=sys.stderr)
        raise SystemExit(1)

    print(f"version {version} is consistent", file=sys.stderr)
    if tag is not None:
        print(f"release tag {tag} matches version {version}", file=sys.stderr)
    return version


def bump_version(new_version: str) -> None:
    if not VERSION_RE.fullmatch(new_version):
        fail(
            f"invalid version {new_version!r}; expected MAJOR.MINOR.PATCH "
            "(optionally -suffix), without a leading v"
        )

    current = section_value(read(CARGO_TOML), "workspace.package", "version", CARGO_TOML)
    if current == new_version:
        fail(f"version is already {new_version}")

    updates = {
        CARGO_TOML: set_section_value(
            read(CARGO_TOML), "workspace.package", "version", new_version, CARGO_TOML
        ),
        APP_CARGO_TOML: set_section_value(
            read(APP_CARGO_TOML),
            "package.metadata.bundle",
            "version",
            new_version,
            APP_CARGO_TOML,
        ),
        CARGO_LOCK: set_lock_versions(read(CARGO_LOCK), new_version),
        README: set_readme_version(read(README), new_version),
        AGENTS: set_agents_version(read(AGENTS), new_version),
        LICENSES: set_license_versions(read(LICENSES), new_version),
    }

    for path, text in updates.items():
        write(path, text)

    version = check_version(None)
    print(f"bumped {current} -> {version}")
    for path in updates:
        print(f"  {relative(path)}")
    print()
    print("next steps:")
    print("  git diff")
    print("  cargo fmt --all --check")
    print("  cargo clippy --workspace --all-targets --locked -- -D warnings")
    print("  cargo test --workspace --locked")
    print(f'  git commit -am "chore: release {version}"')
    print(f'  git tag -a "v{version}" -m "PCB-CAM {version}"')
    print(f'  git push origin master "v{version}"')


def main(argv: list) -> None:
    if argv and argv[0] == "check" and len(argv) in (1, 2):
        print(check_version(argv[1] if len(argv) == 2 else None))
        return
    if len(argv) == 2 and argv[0] == "bump":
        bump_version(argv[1])
        return
    print(__doc__.strip(), file=sys.stderr)
    raise SystemExit(2)


if __name__ == "__main__":
    main(sys.argv[1:])
