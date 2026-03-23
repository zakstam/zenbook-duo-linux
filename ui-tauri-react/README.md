# Zenbook Duo Control UI

## Build commands

- `npm run build`: full Tauri release build with configured bundles.
- `npm run build:local`: release build without packaging bundles. Faster for local verification.
- `npm run build:frontend`: frontend-only production build.
- `npm run build:rust`: Rust-only release build for the Tauri backend.

## Faster builds

The biggest speed win during iteration is to avoid full packaging unless you need `.deb` or `.rpm`.

- Use `npm run build:local` for local release checks.
- Use `npm run build:frontend` when changing only the UI.
- Use `npm run build:rust` when changing only Rust code.
- Use `npm run dev` during active development.

Cargo already uses multiple cores by default. You can cap or tune it explicitly when useful:

```bash
CARGO_BUILD_JOBS=8 npm run build:local
```

If `sccache` is installed, you can enable Rust compile caching:

```bash
RUSTC_WRAPPER=sccache npm run build:local
```

If `mold` is installed on Linux, you can reduce link time with a local Cargo config. An example is provided in `.cargo/config.toml.example`.

To set up the recommended local tooling automatically:

```bash
./scripts/setup-fast-build.sh
```
