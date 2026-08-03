#!/usr/bin/env python3
"""Deploy the neser web app to a remote server via SFTP."""

import argparse
import contextlib
import getpass
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import paramiko

REPO_ROOT = Path(__file__).resolve().parent.parent
DIST_DIR = REPO_ROOT / "dist"
HTACCESS_SRC = REPO_ROOT / "web" / ".htaccess"
REMOTE_BASE = "webroots/www/neser"
PRESERVE_DIRS = {"roms"}


def abort(msg: str) -> None:
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def run(cmd: list[str], **kwargs) -> None:
    result = subprocess.run(cmd, cwd=REPO_ROOT, **kwargs)
    if result.returncode != 0:
        raise RuntimeError(f"Command failed (exit {result.returncode}): {' '.join(cmd)}")


def git_current_ref() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    ref = result.stdout.strip()
    if ref == "HEAD":
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        ref = result.stdout.strip()
    return ref


def check_clean_worktree() -> None:
    result = subprocess.run(["git", "diff", "--quiet"], cwd=REPO_ROOT)
    if result.returncode != 0:
        abort("Working tree has unstaged changes. Commit or stash them first.")
    result = subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=REPO_ROOT)
    if result.returncode != 0:
        abort("Index has staged but uncommitted changes. Commit or stash them first.")


def resolve_tag(tag: str) -> str:
    """Return the actual git tag name, trying both the given name and a 'v' prefix."""
    result = subprocess.run(["git", "tag", "-l", tag], cwd=REPO_ROOT, capture_output=True, text=True)
    if result.stdout.strip():
        return tag
    prefixed = f"v{tag}"
    result = subprocess.run(["git", "tag", "-l", prefixed], cwd=REPO_ROOT, capture_output=True, text=True)
    if result.stdout.strip():
        return prefixed
    raise RuntimeError(f"Tag '{tag}' not found (also tried '{prefixed}')")


def build(tag: str | None = None) -> None:
    if tag is not None:
        resolved = resolve_tag(tag)
        print(f"Checking out tag: {resolved}")
        run(["git", "checkout", resolved])
    else:
        print(f"Building current branch: {git_current_ref()}")

    print("Building web app...")
    run(["bash", "scripts/build_web.sh"])

    if HTACCESS_SRC.exists():
        print("Copying .htaccess into dist/")
        shutil.copy2(HTACCESS_SRC, DIST_DIR / ".htaccess")


def rmtree_sftp(sftp: paramiko.SFTPClient, remote_path: str) -> None:
    """Recursively remove a remote directory."""
    for entry in sftp.listdir_attr(remote_path):
        child = f"{remote_path}/{entry.filename}"
        if stat.S_ISDIR(entry.st_mode):
            rmtree_sftp(sftp, child)
            sftp.rmdir(child)
        else:
            sftp.remove(child)


def clean_remote(sftp: paramiko.SFTPClient, remote_base: str, dry_run: bool = False) -> None:
    """Delete everything in remote_base except PRESERVE_DIRS."""
    print(f"Cleaning remote {remote_base}/ (preserving {PRESERVE_DIRS})...")
    for entry in sftp.listdir_attr(remote_base):
        if entry.filename in PRESERVE_DIRS:
            continue
        child = f"{remote_base}/{entry.filename}"
        if dry_run:
            print(f"[dry-run] Would delete {child}")
            continue
        if stat.S_ISDIR(entry.st_mode):
            rmtree_sftp(sftp, child)
            sftp.rmdir(child)
        else:
            sftp.remove(child)


def upload_dir(sftp: paramiko.SFTPClient, local_path: Path, remote_path: str) -> None:
    """Recursively upload a local directory to the remote path."""
    for item in sorted(local_path.iterdir()):
        remote_item = f"{remote_path}/{item.name}"
        if item.is_dir():
            with contextlib.suppress(OSError):  # directory may already exist
                sftp.mkdir(remote_item)
            upload_dir(sftp, item, remote_item)
        else:
            print(f"  Uploading {item.relative_to(DIST_DIR)} -> {remote_item}")
            sftp.put(str(item), remote_item)


def deploy(username: str, hostname: str, password: str, dry_run: bool = False) -> None:
    print(f"Connecting to {username}@{hostname}...")
    ssh = paramiko.SSHClient()
    ssh.load_system_host_keys()
    ssh.set_missing_host_key_policy(paramiko.RejectPolicy())
    ssh.connect(hostname, username=username, password=password)
    try:
        sftp = ssh.open_sftp()
        try:
            clean_remote(sftp, REMOTE_BASE, dry_run=dry_run)
            if not dry_run:
                print(f"Uploading dist/ -> {REMOTE_BASE}/")
                upload_dir(sftp, DIST_DIR, REMOTE_BASE)
            else:
                print(f"[dry-run] Would upload dist/ -> {REMOTE_BASE}/")
        finally:
            sftp.close()
    finally:
        ssh.close()


def main() -> None:
    parser = argparse.ArgumentParser(description="Deploy the neser web app to a remote server.")
    parser.add_argument("username", help="SSH username for the remote host")
    parser.add_argument("hostname", help="Hostname or IP of the remote server")
    parser.add_argument(
        "tag",
        nargs="?",
        default=None,
        help="Git tag to checkout and deploy (omit to deploy current branch)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be deleted/uploaded without making any changes.",
    )
    args = parser.parse_args()

    if args.tag:
        check_clean_worktree()

    original_ref = git_current_ref() if args.tag else None
    password = getpass.getpass(f"Password for {args.username}@{args.hostname}: ")

    try:
        build(args.tag)
        deploy(args.username, args.hostname, password, dry_run=args.dry_run)
        print("Deployment successful!")
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        if original_ref is not None:
            print(f"Restoring branch: {original_ref}")
            subprocess.run(["git", "checkout", original_ref], cwd=REPO_ROOT)


if __name__ == "__main__":
    main()
