<!-- coderly-ground-note:v1 -->
---
slug: readme-is-user-only-developer-detail-lives-in-agents-md
title: README is user-only; developer detail lives in AGENTS.md
writtenAt: 1792224107710
tablesTouched: 0
askId: 5b8650f5-37b0-4845-863e-5b518f87b48f
files:
  - path: README.md
    blob: b9e3371ccc2296e81473360e7ab03b87059e6538
  - path: AGENTS.md
    blob: 07cff43aacf53e794b4fc3675f6e443c80c9ca8f
---

# README is user-only; developer detail lives in AGENTS.md

README.md is for Duo owners only: support, install, 2026 setup, troubleshooting, uninstall. Architecture, checks, build and maintainer rules go in AGENTS.md (CLAUDE.md just points there). Users have run maintainer commands from the README as root (#28), so keep dev commands out of it. Any README command must match install.sh/setup-common.sh/uninstall.sh; uninstall.sh needs a clone because the one-line installer uses a temp dir.
