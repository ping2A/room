# 0001 — GZDoom reference commit

## Context

Appendix B maps GZDoom source paths to Ferrite crates. Ports and behavioural questions need a pinned upstream revision so two agents do not mix APIs from different years.

## Decision

The reference commit for this repository's start is:

- Repository: <https://github.com/ZDoom/gzdoom>
- Commit: `c26ce2e6ca2a0c770f140cb25dde0d30073ca8f7` (master as of 2026-08-10)

Behavioural ports cite file and function from this commit (or a later commit recorded in a superseding ADR).

## Consequences

- Ambiguous specs are resolved by reading this tree first.
- Bumping the pin is a documented decision, not a silent `master` float.
