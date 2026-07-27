---
name: Feature request
about: Propose an improvement or new capability
title: ""
labels: enhancement
assignees: ""
---

**The problem**

What are you trying to do that Kibitz doesn't support (or makes harder than it
should be)? Concrete workflows beat abstract wishes.

**Proposed solution**

How you imagine it working. Sketches, examples from other tools, or sample
positions welcome.

**Alternatives considered**

Other ways to get the same outcome, and why they fall short.

**Fit check** (saves everyone a round-trip)

- Kibitz keeps the engine off by default — features that require an
  always-on engine are out of scope by principle.
- ChessBase native formats are out of scope (migration path is PGN export).
- Anything touching `crates/*` must stay free of GPL dependencies; see
  CONTRIBUTING.md.
