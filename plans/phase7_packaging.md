# Phase 7: Packaging and Distribution

> **Prerequisites:** Phase 6 complete (all features implemented)
> **Standards:** All code must follow [standards.md](standards.md)
> **New dependencies:** `clap_mangen 0.2`, `clap_complete 4`, `flate2 1` (xtask workspace only — not shipped in binary)

**Goal:** Ship `mdink` as a single binary installable via `curl | sh`, `apt install mdink`,
and `cargo install mdink`. Provide man pages and shell completions.

---

## 7.1 — xtask: Generate Man Page and Shell Completions

### Workspace setup

Add to root `Cargo.toml`:
```toml
[workspace]
members = ["xtask"]
```

### `xtask/Cargo.toml`

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
clap = { version = "4", features = ["derive"] }
clap_mangen = "0.2"
clap_complete = "4"
flate2 = "1"
```

### `xtask/src/main.rs`

Subcommand: `cargo xtask dist-assets`

```rust
// 1. Import the Cli struct from ../src/cli.rs via #[path] include
#[path = "../../src/cli.rs"]
mod cli;

// 2. Generate man page → assets/mdink.1.gz
//    - Use clap_mangen::generate_to() with Cli::command()
//    - Gzip with flate2

// 3. Generate completions → assets/completions/
//    - clap_complete::generate_to(Shell::Bash, ...) → assets/completions/mdink.bash
//    - clap_complete::generate_to(Shell::Zsh, ...)  → assets/completions/_mdink
//    - clap_complete::generate_to(Shell::Fish, ...) → assets/completions/mdink.fish
```

**Standards note:** This is why Phase 1 required `cli.rs` to have zero non-clap dependencies.
The `#[path]` include brings the file into the xtask build — any non-clap imports would
fail to resolve. (See [standards.md §1.2](standards.md))

Run with: `cargo xtask dist-assets`

Output:
```
assets/
├── mdink.1.gz
└── completions/
    ├── mdink.bash
    ├── _mdink
    └── mdink.fish
```

**Files created:** `xtask/Cargo.toml`, `xtask/src/main.rs`

---

## 7.2 — curl Installer Script (`packaging/install.sh`)

A POSIX-compatible shell script following the starship/rustup pattern:

```bash
curl -fsSL https://raw.githubusercontent.com/OWNER/mdink/main/packaging/install.sh | sh
```

### Script behavior

1. **Detect OS:** `uname -s` → `Linux` or `Darwin`
2. **Detect arch:** `uname -m` → `x86_64`/`amd64` or `aarch64`/`arm64`
3. **Resolve version:** use `MDINK_VERSION` env var, or query GitHub API for latest release tag
4. **Construct URL:** `https://github.com/OWNER/mdink/releases/download/v{VERSION}/mdink-v{VERSION}-{ARCH}-{OS}.tar.gz`
5. **Download:** archive + `checksums.txt` (via `curl` or `wget`)
6. **Verify:** SHA-256 checksum (via `sha256sum` or `shasum`)
7. **Extract:** `tar -xzf` → single `mdink` binary
8. **Install:** move to `$MDINK_INSTALL_DIR` (default: `~/.local/bin`), request sudo if needed
9. **PATH check:** warn if install dir is not in `$PATH`

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MDINK_VERSION` | `latest` | Specific version to install (e.g., `v0.2.0`) |
| `MDINK_INSTALL_DIR` | `~/.local/bin` | Installation directory |

### Release archive naming convention

```
mdink-v0.1.0-x86_64-unknown-linux-musl.tar.gz
mdink-v0.1.0-aarch64-unknown-linux-musl.tar.gz
mdink-v0.1.0-x86_64-apple-darwin.tar.gz
mdink-v0.1.0-aarch64-apple-darwin.tar.gz
checksums.txt
```

**Files created:** `packaging/install.sh`

---

## 7.3 — GitHub Actions: Release Workflow

### `.github/workflows/release.yml`

Triggered on pushing a version tag: `v[0-9]+.[0-9]+.[0-9]+`

### Job 1: `build` — Build release binaries (matrix: 4 targets)

| Target | Runner | Method | Purpose |
|--------|--------|--------|---------|
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | `actions-rust-cross` | curl installer (static) |
| `aarch64-unknown-linux-musl` | `ubuntu-latest` | `actions-rust-cross` | curl installer (static) |
| `x86_64-apple-darwin` | `macos-13` | native | curl installer (macOS Intel) |
| `aarch64-apple-darwin` | `macos-14` | native | curl installer (macOS ARM) |

Steps per target:
1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable` with target
3. `cargo xtask dist-assets` (generate man page + completions)
4. `cargo build --profile dist --target $TARGET` (or via `actions-rust-cross`)
5. Package: `tar -czf mdink-{version}-{target}.tar.gz mdink`
6. SHA-256: `sha256sum mdink-*.tar.gz >> checksums.txt`
7. `actions/upload-artifact@v4`

### Job 2: `build-deb` — Build .deb packages (matrix: 2 targets)

For `.deb` packages, use **glibc-linked** builds (not musl) so `$auto` dependency detection works:

| Target | Debian arch |
|--------|-------------|
| `x86_64-unknown-linux-gnu` | `amd64` |
| `aarch64-unknown-linux-gnu` | `arm64` |

Steps:
1. `cargo xtask dist-assets`
2. Build via `cargo-zigbuild` or `actions-rust-cross` (glibc target)
3. `cargo install cargo-deb`
4. `cargo deb --no-build --target $TARGET`
5. Upload `.deb` artifact

### Job 3: `release` — Create GitHub Release (needs: build, build-deb)

1. Download all artifacts
2. Concatenate per-target checksums into one `checksums.txt`
3. `softprops/action-gh-release@v2`:
   - Attach all `.tar.gz` archives
   - Attach all `.deb` files
   - Attach `checksums.txt`

### Job 4: `publish-apt` — Update APT repository (needs: build-deb)

1. Checkout `gh-pages` branch
2. Download `.deb` artifacts
3. Import GPG key from `secrets.GPG_PRIVATE_KEY`
4. `sudo apt-get install -y reprepro`
5. `cd apt-repo && reprepro includedeb stable *.deb`
6. Commit + push to `gh-pages`

**Files created:** `.github/workflows/release.yml`

---

## 7.4 — APT Repository Setup

### One-time manual setup (before first release)

**1. Generate GPG signing key:**
```bash
gpg --batch --gen-key <<EOF
  Key-Type: EdDSA
  Key-Curve: ed25519
  Name-Real: mdink Release Signing Key
  Name-Email: you@example.com
  %no-protection
  %commit
EOF
```

**2. Store private key as GitHub secret:**
```bash
gpg --armor --export-secret-key you@example.com
# → paste into GitHub repo → Settings → Secrets → GPG_PRIVATE_KEY
```

**3. Export public key:**
```bash
gpg --export you@example.com > apt-repo/pubkey.gpg
```

**4. Create `gh-pages` branch with apt repo skeleton:**
```
apt-repo/
├── conf/
│   └── distributions
├── pubkey.gpg
└── index.html              # optional: instructions page
```

**5. `apt-repo/conf/distributions`:**
```
Origin: mdink
Label: mdink
Codename: stable
Suite: stable
Architectures: amd64 arm64
Components: main
Description: mdink APT repository
SignWith: <GPG_FINGERPRINT>
```

**6. Enable GitHub Pages** on the `gh-pages` branch.

### User installation instructions

```bash
# 1. Add GPG key
curl -fsSL https://OWNER.github.io/mdink/apt-repo/pubkey.gpg \
  | sudo gpg --dearmor -o /etc/apt/keyrings/mdink.gpg

# 2. Add repository
echo "deb [signed-by=/etc/apt/keyrings/mdink.gpg] \
  https://OWNER.github.io/mdink/apt-repo stable main" \
  | sudo tee /etc/apt/sources.list.d/mdink.list

# 3. Install
sudo apt update && sudo apt install mdink
```

---

## 7.5 — Version Management

Single source of truth: `version` field in `Cargo.toml`.

| System | How it reads the version |
|--------|------------------------|
| Binary (`mdink --version`) | `clap`'s `#[command(version)]` reads `CARGO_PKG_VERSION` |
| `.deb` package | `cargo-deb` reads `Cargo.toml` version |
| curl installer | Reads the Git tag from GitHub API |
| Man page | `clap_mangen` reads `CARGO_PKG_VERSION` |

### Release process

```bash
# 1. Bump version
vim Cargo.toml  # change version = "0.2.0"

# 2. Commit
git add Cargo.toml Cargo.lock
git commit -m "chore: release v0.2.0"

# 3. Tag
git tag v0.2.0

# 4. Push (triggers release.yml)
git push && git push --tags
```

CI handles everything: build, package, release, apt repo update.

---

## 7.6 — README Installation Section

```markdown
## Installation

### Quick install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/OWNER/mdink/main/packaging/install.sh | sh
```

### APT (Debian / Ubuntu)

```bash
curl -fsSL https://OWNER.github.io/mdink/apt-repo/pubkey.gpg \
  | sudo gpg --dearmor -o /etc/apt/keyrings/mdink.gpg
echo "deb [signed-by=/etc/apt/keyrings/mdink.gpg] \
  https://OWNER.github.io/mdink/apt-repo stable main" \
  | sudo tee /etc/apt/sources.list.d/mdink.list
sudo apt update && sudo apt install mdink
```

### Cargo (from source)

```bash
cargo install mdink
```

### GitHub Releases

Download prebuilt binaries from the [Releases page](https://github.com/OWNER/mdink/releases).
```

---

## 7.7 — crates.io Publishing

Verify `cargo publish --dry-run` succeeds:
- All metadata fields present (name, version, description, license, repository)
- `Cargo.lock` committed
- `LICENSE` file present
- No path dependencies in published crate

---

## Phase 7 — Definition of Done

### xtask and assets
- [ ] `cargo xtask dist-assets` generates `assets/mdink.1.gz`
- [ ] `cargo xtask dist-assets` generates `assets/completions/{mdink.bash, _mdink, mdink.fish}`
- [ ] Man page installs correctly: `man mdink` works after `apt install`
- [ ] Shell completions work for bash, zsh, and fish

### curl installer
- [ ] `packaging/install.sh` works on Linux x86_64
- [ ] `packaging/install.sh` works on Linux aarch64
- [ ] `packaging/install.sh` works on macOS x86_64
- [ ] `packaging/install.sh` works on macOS aarch64
- [ ] Checksum verification succeeds (and fails on tampered archive)
- [ ] `MDINK_VERSION` and `MDINK_INSTALL_DIR` env vars work

### CI/CD
- [ ] `ci.yml` runs on every push/PR (build + test + clippy)
- [ ] `release.yml` triggers on `v*` tags
- [ ] Builds static musl binaries for 2 Linux architectures
- [ ] Builds macOS binaries for 2 architectures
- [ ] Creates GitHub Release with archives + checksums + .deb files

### APT repository
- [ ] GPG key generated and `pubkey.gpg` committed to `gh-pages`
- [ ] `apt-repo/conf/distributions` configured
- [ ] CI builds `.deb` packages for amd64 and arm64
- [ ] CI publishes `.deb` to apt repo via reprepro
- [ ] `apt install mdink` works on a fresh Debian/Ubuntu system
- [ ] `man mdink` works after apt install
- [ ] Shell completions auto-installed by .deb

### General
- [ ] `mdink --version` shows correct version
- [ ] README has installation instructions for curl, apt, and cargo
- [ ] `cargo publish --dry-run` succeeds
- [ ] Phase gate checklist from [standards.md §10](standards.md) passes

**Files created/modified:**
- Created: `xtask/Cargo.toml`, `xtask/src/main.rs`, `packaging/install.sh`, `.github/workflows/release.yml`
- Created (on `gh-pages` branch): `apt-repo/conf/distributions`, `apt-repo/pubkey.gpg`
- Modified: `Cargo.toml` (add `[workspace]`), `README.md`
