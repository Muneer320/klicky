# Pull request

## Problem

Describe the problem this change solves.

## Approach

Explain the implementation and important tradeoffs.

## Verification

List the commands and hardware checks you ran.

```text
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

## Checklist

- [ ] The change is focused and contains no unrelated cleanup.
- [ ] Formatting, check, tests, Clippy, and release build pass.
- [ ] New behavior has focused tests where useful.
- [ ] User-visible behavior is documented.
- [ ] Input or audio claims include real platform evidence.
- [ ] No secrets, local paths, generated output, or private configuration are committed.
- [ ] Public documentation contains no em dashes.
- [ ] Sound assets include redistribution permission.
