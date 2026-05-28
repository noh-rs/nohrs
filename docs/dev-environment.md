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
| **Linux + Docker (対話開発)** | `docker compose -f docker/dev/docker-compose.yml up` (X11 forwarding) |
| **Linux + Docker (headless / CI)** | `docker compose -f docker/ci/docker-compose.yml run nohrs <cmd>` (Xvfb) |
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

### 4.1 docker/dev/ — 対話開発用 (X11 forwarding)

**前提**: Linux host (host の X server を流用)。macOS / Windows host は非推奨。

```text
docker/dev/
├── Dockerfile             # Rust toolchain + gpui 依存をプリインストール
└── docker-compose.yml     # X11 mount, source mount
```

**起動**:
```bash
xhost +local:docker        # host の X server 利用を許可
docker compose -f docker/dev/docker-compose.yml up
# コンテナ内で:
cargo run --features gui --bin nohrs
```

| マウント | 用途 |
|---------|------|
| `/tmp/.X11-unix:/tmp/.X11-unix:rw` | X11 socket |
| `$XAUTHORITY:/root/.Xauthority:ro` | X11 認証 |
| プロジェクトルート | source code |
| `~/.cargo/registry` | Cargo cache の永続化 |
| 環境変数 `DISPLAY` | host の display を渡す |

### 4.2 docker/ci/ — Xvfb headless

CI / AI agent / 自動スクリーンショット用。

```text
docker/ci/
├── Dockerfile             # Xvfb + Rust toolchain + nohrs ビルド依存
└── docker-compose.yml
```

**使用例**:
```bash
docker compose -f docker/ci/docker-compose.yml run nohrs cargo test
docker compose -f docker/ci/docker-compose.yml run nohrs bash script/ui-run.sh shot
```

GitHub Actions では Ubuntu runner で同 image を使用。

---

## 5. Nix (Linux + macOS)

### `flake.nix` (devshell のみ、P1 提供)

```nix
{
  description = "nohrs devshell";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust pkg-config openssl
            libxkbcommon wayland
            fontconfig freetype
            mesa libGL
            cargo-llvm-cov cargo-deny cargo-machete
            typos wrangler
          ];
        };
      });
}
```

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
