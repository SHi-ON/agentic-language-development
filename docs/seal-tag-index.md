# ALD seal-tag index (`pin/*`)

Maps the `pin/*` tags that anchor the paper seal line. Tags are
immutable (the pin-tags-immutable ruleset forbids deletion), so wrong
names stay and are recorded here instead of rewritten.

## Current seal tag

| Tag | Peeled commit | Role |
| --- | --- | --- |
| `pin/f157a856a` | `f157a856a4e3855dac54c151d911a0f21528f7c9` | Canonical seal tag (paper seal) |

## Earlier canonical/alias history

Superseded by the current seal tag above; tags and bytes untouched.

| Tag | Peeled commit | Role |
| --- | --- | --- |
| `pin/f17ea7752` | `f17ea77523ac429b72c44f2536b9486f0fbca82a` | Earlier canonical seal tag |
| `pin/1137e1a9` | `1137e1a879bf0b4f8c78ae563684315ebfbb79c5` | Earlier canonical seal tag |
| `pin/1df2177` | `1137e1a879bf0b4f8c78ae563684315ebfbb79c5` | Alias: same peeled commit, wrong name |
| `pin/f0b6e9f` | `ad2c528fb056046cc7cfc39deefb8be24119f87c` | Prior seal-candidate tag |

## Grandfathered historical tags

Named before the naming rule below existed; kept as-is.

| Tag | Peeled commit |
| --- | --- |
| `pin/c15e5317e` | `c15e5312016bda3dc176ff13a2b48f8752e1cadf` |
| `pin/d6e17e2d9` | `d6e17e2673c676e19f2106207bb0c8c879381abf` |
| `pin/f8547844de` | `f8547844de73d11193191295b3bceee5d2ef051c` |

## Round-numbering note

Tag messages count rounds differently from the paper review log: for
example `pin/1137e1a9` says "round-12 seal" while the paper
`docs/review-log.md` records that seal under "Round 11 addendum".
Tag messages must not be rewritten. The paper review log is
authoritative for round numbering; tag messages are sealed history.

## Pre-push check for future tags

Before pushing a new `pin/*` tag, run:

```sh
corepack pnpm@12.3.4 exec tsx scripts/check-pin-tags.ts
```

The check fails on any `pin/*` tag that is neither named
`pin/<first-9-hex-chars-of-its-peeled-commit>` nor listed above, and
on any listed tag whose peeled commit changed (a history rewrite).
Name every new seal tag `pin/<first-9-hex>` at creation time.
