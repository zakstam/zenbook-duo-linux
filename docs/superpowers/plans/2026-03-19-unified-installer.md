# Unified Installer Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a single `install.sh` entrypoint that auto-detects the supported desktop/session, runs the correct setup script, then installs the optional UI so users can install with one short command.

**Architecture:** Keep the existing `setup-gnome.sh`, `setup-kde.sh`, `setup-niri.sh`, and `install-ui.sh` as the implementation units. Add a thin repo-root wrapper that only handles environment detection, dispatch, and user-facing error messages.

**Tech Stack:** Bash, existing repo installer scripts, README documentation.

---

## Chunk 1: Wrapper Script

### Task 1: Add `install.sh`

**Files:**
- Create: `install.sh`
- Test: manual shell checks

- [ ] **Step 1: Write the failing behavior target**

Target behavior:
- detect GNOME from `XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, or `XDG_SESSION_DESKTOP`
- detect KDE/Plasma from the same env vars
- detect Niri from the same env vars
- stop with a manual fallback message when detection is unknown or conflicting
- pass through extra CLI args to the chosen setup script
- run `install-ui.sh` only if setup succeeds

- [ ] **Step 2: Implement minimal wrapper**

Create `install.sh` as a bash entrypoint that:
- resolves repo-relative paths
- lowercases desktop/session values
- chooses one setup script when signals agree
- prints the chosen backend
- runs the setup script followed by `install-ui.sh`

- [ ] **Step 3: Run shell verification**

Run:
```bash
bash -n install.sh
XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION=gnome XDG_SESSION_DESKTOP=gnome bash install.sh --help
```

Expected:
- syntax check passes
- help text prints without running installers

## Chunk 2: Docs

### Task 2: Update install docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add the new recommended install path**

Document:
- `./install.sh` as the preferred local install entrypoint
- the short `curl` one-liner that downloads and runs `install.sh`
- compositor-specific setup scripts as manual fallbacks

- [ ] **Step 2: Verify doc examples match script behavior**

Run:
```bash
rg -n "install.sh|setup-gnome|setup-kde|setup-niri" README.md
```

Expected:
- README presents `install.sh` first
- fallback commands remain available

## Chunk 3: Final Verification

### Task 3: Validate the unified flow

**Files:**
- Verify: `install.sh`, `README.md`

- [ ] **Step 1: Run syntax and detection smoke tests**

Run:
```bash
bash -n install.sh
env XDG_CURRENT_DESKTOP=GNOME DESKTOP_SESSION= XDG_SESSION_DESKTOP= bash install.sh --help
env XDG_CURRENT_DESKTOP=KDE DESKTOP_SESSION= XDG_SESSION_DESKTOP= bash install.sh --help
env XDG_CURRENT_DESKTOP=niri DESKTOP_SESSION= XDG_SESSION_DESKTOP= bash install.sh --help
```

Expected:
- all commands succeed
- help mode exits cleanly

- [ ] **Step 2: Summarize user-facing one-liner**

Final one-liner:
```bash
bash <(curl -fsSL https://raw.githubusercontent.com/zakstam/zenbook-duo-linux/main/install.sh)
```
