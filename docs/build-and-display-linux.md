# Linux でのビルド・表示手順書

Nohrs は README 上 macOS 向けだが、GUI フレームワーク gpui は Linux にも対応している。
この手順書は **GPU の無い Linux（ソフトウェア Vulkan = llvmpipe）+ VNC ディスプレイ** 環境で、
GUI バイナリをビルドして画面に表示するまでの再現手順をまとめたもの。

検証環境:

- OS: Debian GNU/Linux 13 (trixie)
- Rust: stable（`rust-toolchain.toml` で固定）
- GPU: 無し（Mesa の llvmpipe によるソフトウェア Vulkan で描画）
- ディスプレイ: noVNC 経由の Xvnc（例: `DISPLAY=:108`、1280x800）
- 権限: `sudo` にパスワードが必要 → **root 不要の手順で構成**

---

## 0. 前提パッケージの確認

ランタイムの Vulkan は導入済みであること（多くの環境で既に入っている）。

```bash
ls /usr/lib/x86_64-linux-gnu/libvulkan_lvp.so   # llvmpipe（ソフトウェア Vulkan）
which vulkaninfo
```

`libvulkan_lvp.so` があれば GPU 無しでも描画できる。

---

## 1. リンク用ライブラリの用意（root 不要）

gpui の Linux バックエンドは `xcb` / `xkbcommon` / `xkbcommon-x11` / `wayland` / `vulkan`
にリンクする。dev パッケージ（`lib*-dev`）が無いと、リンク時に
`unable to find library -lxcb` などのエラーになる。

dev パッケージを入れられない（`sudo` 不可）場合は、既存のランタイム `.so` に対して
**非バージョン付きの symlink** を書き込み可能なディレクトリに作り、リンカの検索パスに渡す。

```bash
D="$HOME/.local/devlibs"
mkdir -p "$D"
LIB=/usr/lib/x86_64-linux-gnu
ln -sf $LIB/libxcb.so.1            "$D/libxcb.so"
ln -sf $LIB/libxkbcommon.so.0      "$D/libxkbcommon.so"
ln -sf $LIB/libxkbcommon-x11.so.0  "$D/libxkbcommon-x11.so"
ln -sf $LIB/libwayland-client.so.0 "$D/libwayland-client.so"
ln -sf $LIB/libwayland-cursor.so.0 "$D/libwayland-cursor.so"
ln -sf $LIB/libwayland-egl.so.1    "$D/libwayland-egl.so"
ln -sf $LIB/libvulkan.so.1         "$D/libvulkan.so"
```

> root が使えるなら、symlink の代わりに dev パッケージを入れるのが本筋:
> ```bash
> sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
>                         libwayland-dev libvulkan-dev
> ```
> この場合は手順 1 の symlink と手順 2 の `RUSTFLAGS` は不要。

---

## 2. ビルド

GUI は `gui` フィーチャー付きの `nohrs` バイナリ。手順 1 の symlink ディレクトリを
`RUSTFLAGS` でリンカに渡す。

```bash
RUSTFLAGS="-L $HOME/.local/devlibs" cargo build --features gui --bin nohrs
```

成功すると `target/debug/nohrs` が生成される。

---

## 3. 表示（VNC ディスプレイ上で起動）

接続先の VNC ディスプレイ番号を確認する。

```bash
ps aux | grep -i Xvnc | grep -v grep   # 例: "Xvnc :108 ..." の :108 が表示先
```

そのディスプレイを指定して起動する。

```bash
DISPLAY=:108 target/debug/nohrs
```

GPU が無い環境では llvmpipe で描画するため、**起動直後の数秒はウィンドウが黒いまま**
になる（フォントアトラスや UI 要素の生成待ち）。ログに以下が出れば描画準備完了:

```text
blade_graphics::hal::init: Adapter: "llvmpipe (...)"   # ソフトウェア Vulkan を使用
... name 'atlas' ...                                    # フォントアトラス生成
gpui::platform::linux::x11::client: Refreshing every 16ms
```

数秒待つと noVNC 側にファイルエクスプローラの UI（サイドバー / ファイル一覧 / プレビュー枠）
が表示される。

停止:

```bash
pkill -x nohrs
```

---

## 4. （任意）ヘッドレスでスクリーンショットを取得

noVNC を開かずに描画結果を確認したい場合。`xwd` で X 画面をダンプし、PNG に変換する。
（`xwd` は通常入っている。`convert`/`ffmpeg` 等が無くても下記の Python で PNG 化できる）

`xwd2png.py`（標準ライブラリの zlib のみ使用）:

```python
import sys, struct, zlib

def read_xwd(path):
    with open(path, "rb") as f:
        data = f.read()
    fields = struct.unpack(">25I", data[:100])
    (header_size, _ver, _fmt, _depth, w, h, _xoff, byte_order, _bu, _bbo,
     _pad, bpp_bits, bytes_per_line, _vc, red_mask, green_mask, blue_mask,
     _bpr, _cme, ncolors, *_rest) = fields
    pixels = data[header_size + ncolors * 12:]

    def mask_shift(mask):
        if mask == 0: return 0, 0
        shift = 0
        while not (mask >> shift) & 1: shift += 1
        width = 0
        while (mask >> (shift + width)) & 1: width += 1
        return shift, width

    rsh, rw = mask_shift(red_mask); gsh, gw = mask_shift(green_mask); bsh, bw = mask_shift(blue_mask)
    bpp = bpp_bits // 8
    rows = bytearray()
    for y in range(h):
        rows.append(0)  # PNG filter "none"
        base = y * bytes_per_line
        for x in range(w):
            chunk = pixels[base + x * bpp: base + x * bpp + bpp]
            px = int.from_bytes(chunk, "little" if byte_order == 0 else "big") if len(chunk) == bpp else 0
            r = (px & red_mask) >> rsh; g = (px & green_mask) >> gsh; b = (px & blue_mask) >> bsh
            if 0 < rw < 8: r <<= 8 - rw
            if 0 < gw < 8: g <<= 8 - gw
            if 0 < bw < 8: b <<= 8 - bw
            rows += bytes((r & 0xFF, g & 0xFF, b & 0xFF))
    return w, h, bytes(rows)

def write_png(path, w, h, raw):
    def chunk(tag, payload):
        c = tag + payload
        return struct.pack(">I", len(payload)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))

if __name__ == "__main__":
    w, h, raw = read_xwd(sys.argv[1]); write_png(sys.argv[2], w, h, raw)
```

取得手順（起動・描画待ち・キャプチャを 1 コマンドで完結させる）:

```bash
DISPLAY=:108 setsid target/debug/nohrs > /tmp/nohrs.log 2>&1 < /dev/null &
# 描画開始までログ上のマーカーを待つ
timeout 20 bash -c 'until grep -q "Refreshing every" /tmp/nohrs.log; do :; done'
perl -e 'select(undef,undef,undef,4)'   # UI 描画の落ち着き待ち（sleep 代替）
DISPLAY=:108 xwd -root -silent -out /tmp/nohrs.xwd
python3 xwd2png.py /tmp/nohrs.xwd /tmp/nohrs.png
```

---

## トラブルシューティング

| 症状 | 原因 / 対処 |
| --- | --- |
| `unable to find library -lxcb` 等 | 手順 1 の symlink、または `lib*-dev` パッケージが不足。`RUSTFLAGS="-L ..."` を付けたか確認 |
| ウィンドウが黒いまま | llvmpipe での初回描画が遅いだけ。数秒待つ。ログに `'atlas'` / `Refreshing every 16ms` が出れば描画中 |
| `pkill -f target/debug/nohrs` で操作中シェルごと落ちる | `-f` がそのパス文字列を含む自分のシェルにマッチするため。**`pkill -x nohrs`（プロセス名で厳密一致）を使う** |
| `Found no xinput mouse pointers`（ERROR ログ） | 仮想ディスプレイにポインタが無いだけで、描画・起動には影響しない |
| GUI が無反応 / 重い | GPU 無し（ソフトウェア Vulkan）のため。描画は遅いが動作はする |

---

## 補足: ビルドに必要だったコード修正

ロック済みの `gpui-component 0.3.1` に存在しない API を使っている箇所があり、
そのままでは lib がコンパイルできない（`src/pages/explorer/view/preview/editor.rs`）。
公開 API に合わせて以下を修正済み:

- `InputState::scroll_to`（private）→ 公開の `set_cursor_position` + `RopeExt::offset_to_position` で代替
- `InputState::set_search_query`（0.3.1 に存在せず）→ no-op 化。`on_safe_search` は組み込みの
  `gpui_component::input::Search` アクションのディスパッチに変更

プレビュー内検索ハイライトの連携を本格対応するには、当該 API を持つ gpui-component への更新が必要。
