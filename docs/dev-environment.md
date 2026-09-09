# Development Environment

> Status: Draft (P1 で構築)
> Related: [`ROADMAP.md`](./ROADMAP.md), [`docs/agent-ui-verification.md`](./agent-ui-verification.md), [`docs/build-and-display-linux.md`](./build-and-display-linux.md)

本書は nohrs の開発環境セットアップを定めます。OS と用途に応じた推奨セットアップを示します。

---

## 1. 推奨セットアップ早見表

| 環境 | 推奨手段 |
|------|---------|
| **macOS native** | `rustup` + Xcode + Metal toolchain |
| **Linux native** | `rustup` + apt/pacman で gpui 依存 |
| **Linux + Nix** | `nix develop` (devshell) |
| **Linux + Docker (対話開発)** | `docker compose -f docker/dev/docker-compose.gpu.yml up` (GPU) / `docker-compose.vnc.yml up` (CPU) |
| **Linux + Docker (headless / CI)** | `docker compose -f docker/ci/docker-compose.yml run --rm nohrs <cmd>` (Xvnc) |
| **macOS host + Docker** | **非推奨**。XQuartz 経由は遅い。native 開発を |
| **Windows** | **非推奨**。WSL2 + Linux native か WSL2 + Docker |

---

## 2. macOS native

詳細手順は README に記載。要点:

1. Xcode App Store からインストール
2. `xcode-select --install`
3. `sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer`
4. `xcodebuild -downloadComponent MetalToolchain` (Metal が見つからないと出た場合)
5. `rustup` (toolchain は `rust-toolchain.toml` で固定)
6. `cargo build --features gui` で確認

---

## 3. Linux native

### Ubuntu / Debian

```bash
sudo apt-get install -y \
    build-essential pkg-config \
    libxkbcommon-dev libwayland-dev \
    libxcb-shape0-dev libxcb-xfixes0-dev \
    libfontconfig1-dev libfreetype6-dev \
    libgl1-mesa-dev libegl1-mesa-dev \
    libssl-dev libsqlite3-dev
```

### Arch

```bash
sudo pacman -S --needed base-devel pkgconf \
    libxkbcommon wayland \
    fontconfig freetype2 \
    mesa libgl
```

### 起動

```bash
cargo run --features gui --bin nohrs
```

スクリーンショット / ヘッドレス確認は [`docs/agent-ui-verification.md`](./agent-ui-verification.md) 参照。

---

## 4. Docker (Linux host)

### 前提条件

- ユーザーを `docker` グループに追加済み: `sudo usermod -aG docker $USER`（変更後は再ログイン）
- GPU パススルーを使用する場合は `nvidia-container-toolkit` がインストール済みであること

### 動作対応表

| 環境 | GPU (NVIDIA PT) | CPU (llvmpipe) |
|------|-----------------|----------------|
| **dev X11 forwarding** | ✅ `docker-compose.gpu.yml` | ❌ UI描画されず |
| **dev VNC 接続** | ✅ `docker-compose.vnc.yml` | ✅ `docker-compose.vnc.yml` |
| **ci (test)** | ✅ | ✅ |
| **ci (screenshot)** | ✅ | ✅ |

### 4.1 docker/dev/ — 対話開発用

**前提**: Linux host。macOS / Windows host は非推奨。

```text
docker/dev/
├── Dockerfile                # Rust toolchain + gpui 依存をプリインストール
├── docker-compose.gpu.yml    # NVIDIA GPU パススルー（X11 forwarding）
├── docker-compose.vnc.yml    # TigerVNC + fvwm（GPU なし / llvmpipe 用）
└── docker-compose.yml        # X11 forwarding（llvmpipe ⚠ UI 描画されず）
```

**起動**:
```bash
# GPU あり（X11 forwarding、推奨）
xhost +local:docker
docker compose -f docker/dev/docker-compose.gpu.yml up

# GPU なし（VNC 接続）
docker compose -f docker/dev/docker-compose.vnc.yml up
# → VNC クライアントで localhost:5999 に接続
```

> **注意**: `docker-compose.yml`（X11 forwarding + llvmpipe）は GPUI の Vulkan WSI present がホスト X11 で正しく動作しないため、GPU なし環境では `docker-compose.vnc.yml` を使用してください。

### 4.2 docker/ci/ — Xvnc headless

CI / AI agent / 自動スクリーンショット用。TigerVNC + fvwm を使用し、Xvfb の黒画面問題を回避します。

```text
docker/ci/
├── Dockerfile                # TigerVNC + fvwm + Rust toolchain + スクショツール
├── docker-compose.yml        # TigerVNC + fvwm + llvmpipe（GPU なし）
└── docker-compose.gpu.yml    # NVIDIA GPU パススルー付き
```

**使用例**:
```bash
# GPU あり（cargo test）
docker compose -f docker/ci/docker-compose.gpu.yml run --rm nohrs cargo test

# GPU なし（cargo test）
docker compose -f docker/ci/docker-compose.yml run --rm nohrs cargo test

# スクリーンショット
docker compose -f docker/ci/docker-compose.gpu.yml run --rm \
  nohrs bash -c "cargo build -p nohrs && script/ui-run.sh shot /nohrs/screenshot.png"
```

GitHub Actions では Ubuntu runner で同 image を使用（#50 と連携）。

---

## 5. Nix (Linux + macOS)

### `flake.nix` (devshell のみ、P1 提供)

リポジトリルートの `flake.nix` が devshell を提供する。

**inputs**:
- `nixpkgs` (nixos-unstable) + `rust-overlay` + `flake-utils`
- Rust toolchain は `rust-toolchain.toml` から自動取得（`rust-bin.fromRustupToolchainFile`）

**buildInputs（共通）**: `rust`, `pkg-config`, `openssl`, `fontconfig`, `freetype`, `cargo-llvm-cov`, `cargo-deny`, `cargo-machete`, `typos`

**buildInputs（Linux 追加）**: `libxkbcommon`, `wayland`, `mesa`, `libGL`, `libxcb`, `libx11`, `libxcursor`, `libxi`, `vulkan-loader`, `vulkan-headers`
- macOS では Metal を使うためこれらは不要。`lib.optionals stdenv.isLinux` で条件付き。
- `LD_LIBRARY_PATH` に `makeLibraryPath` で生成したパスを設定し、実行時リンクを解決。

**direnv（任意）**: `.envrc`（`use flake`）をコミット済み。`direnv allow` で `cd` 時に自動ロード。

**使用**:
```bash
nix develop
cargo build --features gui
```

### `nix build` (P5 以降に検討)

devshell のみで開始し、reproducible build (`nix build`) は将来検討。crane / naersk + gpui の vendoring 設計が必要。

---

## 6. エディタ / IDE

| エディタ | 推奨 |
|---------|------|
| VS Code | rust-analyzer 拡張 + Even Better TOML + Pagefind (docs 用 web 開発時) |
| RustRover / IntelliJ | Rust plugin |
| Zed | LSP (`rust-analyzer`) は標準搭載 |

`.vscode/extensions.json` で推奨拡張を列挙 (P2 で整備)。

---

## 7. AI agent 開発支援

リポジトリ ルートに以下を配備 (一部既存):

- `CLAUDE.md` — Claude Code / Claude API 向けプロジェクトガイド
- `AGENTS.md` — 汎用 AI agent ガイド
- `.factory/skills/` (将来) — 開発スキル定義
- `.claude/commands/` (将来) — Slash コマンド定義

詳細はプラグインテンプレ ([`docs/plugin-templates.md`](./plugin-templates.md)) 側の MCP / skill 提供を参照。

---

## 8. リポジトリ取得

```bash
git clone https://github.com/noh-rs/nohrs.git
cd nohrs
# 環境別に上記セクションのいずれか
```

PR 開発時のブランチ規約は [`ROADMAP.md`](./ROADMAP.md) §29-F を参照。
