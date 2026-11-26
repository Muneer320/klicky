# CherryPick contributions

[Muneer320/klicky](https://github.com/Muneer320/klicky) is the canonical repository
for Klicky, releases and the website. The
[CherryPick fork](https://github.com/CherryPick-Student-Projects/klicky) is the
contribution entry point for the college club. Students open PRs there so the
club can track their work. Both use `master` as their default branch.

## Find and link work

Use the [shared contribution board](https://github.com/users/Muneer320/projects/1)
to find issues and PRs across both repositories. Board entries point to the real
GitHub items; comments, reviews and authorship remain in their source repositories.

Before starting, search both repositories and agree on the scope. If an issue
already exists upstream, create a focused club issue only when needed for club
tracking. Include `Related to https://github.com/Muneer320/klicky/issues/NUMBER`,
and add the club issue's URL to the original issue. Link both from the club PR.
Use the new issue forms' optional **Related issue** field for these links.

A club PR can close its club issue. Reference the upstream issue with **Related
to**, rather than a closing keyword, until the fix is actually accepted upstream.
The upstream integration PR can then close the original issue after its complete
acceptance criteria pass. A merge into the fork alone does not resolve an
upstream issue or make a feature available in a release.

Keep discussions on the original item instead of copying every comment or review.
For work available in both repositories, check the linked issue before assigning
someone else to avoid two people independently implementing the same change.

## How code reaches both repositories

1. Create a feature branch and open a PR against the club fork.
2. Review it and pass the relevant checks. All merges remain manual.
3. The original repository's sync workflow notices incoming club commits and
   opens one integration PR from `CherryPick-Student-Projects/klicky:master`
   to `Muneer320/klicky:master`.
4. Review that PR and use **Create a merge commit**. Do not squash or rebase an
   integration PR: the repositories need shared commit ancestry.
5. The same workflow proposes incoming upstream changes from
   `Muneer320/klicky:master` to the fork's `master`. Review and merge that PR with
   a merge commit too.

Feature PRs can still use the normal squash-merge convention. The merge-commit
requirement applies to repository integration PRs, preserving feature authors and
their commits without repeatedly proposing changes that were already integrated.

The workflow runs only in the original repository, checking both directions on
pushes to `master`, roughly every six hours, and on manual dispatch. The schedule
is best effort; GitHub may delay it. Incoming changes are
fetched as Git objects and never checked out or executed by the sync job. Only
the two configured repositories and their `master` branches are eligible. The
job opens a cross-repository PR and never pushes a branch, merges, force-pushes,
copies tags or publishes anything. The head stays in its source repository so
unreviewed fork workflows do not gain same-repository access to upstream secrets.

Equal file trees produce no new PR, even when merge commits differ.
The source must also contain file changes since the common ancestor. A unique
merge commit with no such changes does not require a proposal, even when the
destination has moved ahead and the two current file trees differ.

GitHub keeps an existing PR's head current as the source branch advances,
including reversions.
Repeated checks reuse that open PR. Closing a sync PR without merging suppresses
another proposal for the same source revision; later source revisions can be
proposed. Resolve conflicts through a reviewed change on the source side or a
separate integration PR. Keep force pushes disabled on both default branches and
do not squash an integration PR to hide conflicts.

## Automation setup

Review and merge the setup PR in **both** repositories using merge commits.
Before activation, confirm that both default branches require PRs and passing CI,
and that merge commits are allowed. Keep the default Actions token read-only and
the Actions setting for creating/approving PRs unchanged. The automation uses a
dedicated GitHub App to open PRs without enabling that combined setting. It never
copies workflow files into local branches or needs permission to write them.

Install the dedicated sync App on **only** `Muneer320/klicky` and
`CherryPick-Student-Projects/klicky`, with repository **Pull requests** set to
read/write, plus mandatory Metadata read access. Grant no Contents, Workflows,
organization, administration, secrets, member, issue or project permissions. The
club organization administrator may need to approve its installation. Do not add
the App to branch-protection bypass lists or give it release credentials. GitHub
combines PR creation, editing and review in this permission; protect the key and
review automation changes carefully. The script uses only list/create PR APIs.

Store the App's client ID as the `CLUB_SYNC_CLIENT_ID` Actions variable and its
private key as the `CLUB_SYNC_PRIVATE_KEY` Actions secret **only in the original
repository**. No secret belongs in the fork or a source file. Each matrix job
mints a short-lived token restricted to its destination's single `klicky`
repository; the official token action revokes it after the job. The key can mint
PR access to both repositories, so it is a cross-repository credential. It never
approves or merges PRs. Missing App configuration fails explicitly. The protected
default branch is the only source of executable sync code. Proposed fork changes
run under GitHub's normal fork CI restrictions and still require review.

Enable normal CI in the fork, retaining contributor-run approvals.
The release-candidate, Store-package and APT/Pages workflows are restricted to
`Muneer320/klicky`. Do not copy release credentials into the fork or enable a
second production Pages site. Ordinary CI can build test artifacts in either repo.
Review workflow changes before merging; these guards do not replace GitHub
repository permissions or protect against an administrator changing the workflow.

Configure the shared board's native **Auto-add to project** workflows for each
repository, with `is:issue,pr is:open`. These add new or updated items by reference,
without copying text or changing issue or PR state. GitHub keeps the linked items'
titles and states current afterward. Backfill existing open items separately;
auto-add only reacts to future creation or updates. Keep automatic issue closure
disabled. This uses GitHub's native automation and needs no project token or
cross-repository credential. The number of auto-add workflows depends on the
account plan; this setup needs two.

After activation, run **Propose repository sync** manually in the original repository.
An identical pair should report no incoming file changes. The next reviewed
contribution should open one upstream integration PR; repeated runs must not
create duplicates. Verify CI actually runs on bot-created PRs and approve pending
runs through GitHub when required. Do not merge an unchecked PR.

## Diagnose activation failures

Run `python3 scripts/sync-club.py --repository Muneer320/klicky --dry-run` and
repeat with `--repository CherryPick-Student-Projects/klicky` to inspect both
directions without creating PRs. These checks fetch Git objects and list PRs;
they do not prove that the sync App can create a real proposal or trigger CI.

If App token creation reports 404 for the club repository installation, a club
administrator must approve the App for only that repository. Repository maintain
access does not replace organization installation approval. Keep the App's
existing PR-only permissions and the private key in the canonical repository.

The script reports failed API operations with GitHub's message and validation
details. Check those before retrying: an empty comparison requires no proposal;
an installation or permission failure requires correcting that setup. Do not
fall back to a personal token, broaden App permissions, copy incoming workflows
onto a trusted branch, or enable automatic approval/merging to bypass a failure.

A club administrator must also require the applicable CI checks on the club
default branch. Keep both tracking issues open until real App-created proposals
and their CI have been verified in both directions. A no-change result or a local
test alone does not complete activation.
