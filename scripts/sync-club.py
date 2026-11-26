#!/usr/bin/env python3
"""Propose default-branch integration without merging or executing incoming code."""

import argparse
import json
import os
import subprocess
import sys
from urllib.parse import urlencode


REPOSITORIES = {
    "Muneer320/klicky": "CherryPick-Student-Projects/klicky",
    "CherryPick-Student-Projects/klicky": "Muneer320/klicky",
}
MARKER = "<!-- klicky-club-sync -->"


def command(*args):
    return subprocess.check_output(args, text=True, encoding="utf-8").strip()


def api(path, data=None):
    args = ["gh", "api", path]
    if data is not None:
        args.extend(["--method", "POST", "--input", "-"])
    result = subprocess.run(
        args, input=json.dumps(data) if data is not None else None,
        text=True, encoding="utf-8", capture_output=True,
    )
    if result.returncode:
        try:
            response = json.loads(result.stdout)
            detail = {key: response[key] for key in ("message", "errors", "status")
                      if key in response}
        except (ValueError, TypeError):
            detail = result.stderr.strip()
        # Keep errors useful without logging request bodies, headers or tokens.
        detail = json.dumps(detail, ensure_ascii=True)
        for variable in ("GH_TOKEN", "GITHUB_TOKEN"):
            token = os.environ.get(variable)
            if token:
                detail = detail.replace(token, "[redacted]")
        method = "POST" if data is not None else "GET"
        raise RuntimeError(f"GitHub API {method} {path.split('?')[0]} failed: {detail[:2000]}")
    return json.loads(result.stdout)


def sync(repository, dry_run=False):
    source = REPOSITORIES[repository]
    source_owner = source.split("/")[0]
    if command("git", "rev-parse", "--is-shallow-repository") == "true":
        raise RuntimeError("Full Git history is required; use checkout fetch-depth: 0.")
    # Explicit refspecs never fetch tags or modify a default branch. Incoming
    # files remain Git objects; no checkout, build or hooks from the source run.
    for name, remote in (("base", repository), ("source", source)):
        command("git", "fetch", "--no-tags", f"https://github.com/{remote}.git",
                f"+refs/heads/master:refs/klicky-sync/{name}")
    base = command("git", "rev-parse", "refs/klicky-sync/base")
    head = command("git", "rev-parse", "refs/klicky-sync/source")
    ahead = int(command("git", "rev-list", "--count", f"{base}..{head}"))
    head_tree = command("git", "rev-parse", f"{head}^{{tree}}")
    merge_base = command("git", "merge-base", base, head)
    incoming = (ahead > 0
                and head_tree != command("git", "rev-parse", f"{base}^{{tree}}")
                and head_tree != command("git", "rev-parse", f"{merge_base}^{{tree}}"))

    query = urlencode({"state": "all", "base": "master", "head": f"{source_owner}:master",
                       "per_page": 100, "sort": "created", "direction": "desc"})
    pulls = [pr for pr in api(f"repos/{repository}/pulls?{query}")
             if pr["head"]["repo"] and pr["head"]["repo"]["full_name"] == source]
    opened = [pr for pr in pulls if pr["state"] == "open"]
    if opened:
        # The head stays in its original repository. GitHub updates the diff as
        # master advances, including reversions; never promote fork workflows to
        # a trusted local branch before their review.
        print(f"Existing cross-repository sync PR follows source master: {opened[0]['html_url']}")
        return
    if not incoming:
        print("No incoming file changes; no sync PR needed.")
        return
    if pulls and not pulls[0].get("merged_at") and pulls[0]["head"]["sha"] == head:
        print("This revision's sync PR was closed without merging; leaving it closed.")
        return

    print(f"Propose {source}@{head} -> {repository}:master ({ahead} incoming commits).")
    if dry_run:
        print("Dry run: no remote branch or PR changed.")
        return
    body = (
        f"{MARKER}\n"
        f"Bring default-branch changes from [{source}](https://github.com/{source}) "
        f"into `{repository}:master`.\n\n"
        "Review the diff, CI and the source PRs before merging. Use **Create a merge commit** "
        "to preserve shared history; do not squash or rebase this integration PR. "
        "Resolve conflicts on the source side or through a separately reviewed integration branch. "
        "The head remains in the source repository to preserve GitHub's fork trust boundary.\n\n"
        "This PR does not merge itself, close upstream issues, publish releases or deploy the site. "
        "Reference the original issues when their fixes are accepted upstream.\n\n"
        "[Shared contribution board](https://github.com/users/Muneer320/projects/1)"
    )
    pr = api(f"repos/{repository}/pulls", {
        "title": f"chore: sync changes from {source.split('/')[0]}",
        "head": f"{source_owner}:master", "head_repo": "klicky", "base": "master", "body": body,
    })
    print(f"Created sync PR: {pr['html_url']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--repository", choices=REPOSITORIES,
                        default=os.environ.get("GITHUB_REPOSITORY", ""))
    args = parser.parse_args()
    repository = args.repository
    if repository not in REPOSITORIES:
        parser.error("Run only in one of the two configured Klicky repositories.")
    sync(repository, args.dry_run)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Sync stopped: {error}", file=sys.stderr)
        sys.exit(1)
