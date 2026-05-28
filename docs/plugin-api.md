# Plugin API — WIT World

> Status: Draft (P4 で実装)
> Related: [`docs/plugin-overview.md`](./plugin-overview.md), [`docs/plugin-permissions.md`](./plugin-permissions.md), [`docs/persistence.md`](./persistence.md)

本書は WIT (WebAssembly Interface Type) で表現するプラグイン API の表面を定めます。`nohrs:plugin@0.1.0` がプラグイン作者向けの正式 world です。

---

## 1. WIT World

```wit
// crates/nohrs-plugin-host/wit/world.wit
package nohrs:plugin@0.1.0;

world plugin {
  // ============================================
  // Host imports (plugin が host を呼ぶ)
  // ============================================
  import logging;
  import kv;
  import cache;
  import metadata;
  import fs;
  import network;
  import process;
  import clipboard;
  import notification;
  import launcher;
  import explorer;
  import search;

  // ============================================
  // Plugin exports (host が plugin を呼ぶ)
  // ============================================
  export commands;
  export decorations;
  export previews;
  export events;
}
```

---

## 2. Host imports

各 interface は permission に応じて利用可否が決まります。permission は [`docs/plugin-permissions.md`](./plugin-permissions.md) を参照。

### 2.1 logging (常時許可)

```wit
interface logging {
  enum level { trace, debug, info, warn, error }
  log: func(lvl: level, msg: string);
}
```

### 2.2 kv (常時許可、plugin 専用)

詳細は [`docs/persistence.md`](./persistence.md) §plugin KV。redb-backed、plugin_id ごとにテーブル隔離。

```wit
interface kv {
  variant kv-op {
    put(tuple<string, list<u8>>),
    delete(string),
  }
  get:         func(key: string) -> option<list<u8>>;
  put:         func(key: string, value: list<u8>);
  delete:      func(key: string);
  list-prefix: func(prefix: string) -> list<tuple<string, list<u8>>>;
  batch:       func(ops: list<kv-op>);
}
```

### 2.3 cache (常時許可、TTL 付き)

```wit
interface cache {
  get:          func(key: string) -> option<list<u8>>;
  put-with-ttl: func(key: string, value: list<u8>, ttl-seconds: u64);
  delete:       func(key: string);
  clear:        func();
}
```

### 2.4 metadata (read_paths 範囲内)

読み取り専用のファイルメタデータ問い合わせ。詳細は [`docs/persistence.md`](./persistence.md) §`MetadataQuery`。

```wit
interface metadata {
  record file-meta {
    path:        string,
    parent-path: string,
    size:        u64,
    mtime-ns:    u64,
    inode:       u64,
    is-dir:      bool,
    is-symlink:  bool,
  }
  list-children:  func(path: string) -> list<file-meta>;
  get-file:       func(path: string) -> option<file-meta>;
  find-by-name:   func(pattern: string, limit: u32) -> list<file-meta>;
}
```

### 2.5 fs (read_paths / write_paths permission)

```wit
interface fs {
  variant fs-error { not-permitted, not-found, io-error(string) }
  read-file:    func(path: string) -> result<list<u8>, fs-error>;
  read-text:    func(path: string) -> result<string, fs-error>;
  read-dir:     func(path: string) -> result<list<string>, fs-error>;
  // write 系は write_paths permission 必須
  write-file:   func(path: string, content: list<u8>) -> result<_, fs-error>;
  delete-file:  func(path: string) -> result<_, fs-error>;
  create-dir:   func(path: string) -> result<_, fs-error>;
}
```

### 2.6 network (network permission、ドメイン allowlist)

```wit
interface network {
  record http-request {
    url:     string,
    method:  string,      // "GET" | "POST" | ...
    headers: list<tuple<string, string>>,
    body:    option<list<u8>>,
  }
  record http-response {
    status:  u16,
    headers: list<tuple<string, string>>,
    body:    list<u8>,
  }
  http-fetch: func(req: http-request) -> result<http-response, string>;
}
```

raw TCP/UDP は提供しない (HTTP only)。リダイレクトは同一 host のみ追従。

### 2.7 process (process permission、コマンド名 allowlist)

```wit
interface process {
  record spawn-output {
    exit-code: s32,
    stdout:    list<u8>,
    stderr:    list<u8>,
  }
  spawn: func(cmd: string, args: list<string>) -> result<spawn-output, string>;
}
```

`sh -c` 直渡しは禁止 (host 側で reject)。`args` は必ず list。

### 2.8 clipboard (clipboard permission)

```wit
interface clipboard {
  read-text:  func() -> option<string>;
  write-text: func(text: string);
}
```

### 2.9 notification (常時許可、レート制限あり)

```wit
interface notification {
  show: func(title: string, body: string);
}
```

レート制限: 1 plugin あたり 10 件/分。超過は静かに drop。

### 2.10 launcher (launcher 文脈で使用)

```wit
interface launcher {
  push-view:   func(view: view-node);
  set-title:   func(title: string);
  set-loading: func(loading: bool);
  pop:         func();   // ナビスタックを 1 段戻す
}
```

`view-node` は §UI レンダリングモデル 参照。

### 2.11 explorer (decorator/preview contribute 時に使用)

```wit
interface explorer {
  reveal:        func(path: string);   // explorer で当該ファイルにフォーカス
  open-with:     func(path: string, app: string);
}
```

### 2.12 search (P4 で search-V3 完成後に有効)

```wit
interface search {
  record search-hit {
    path:    string,
    score:   f32,
    snippet: option<string>,
  }
  search-files:   func(query: string, limit: u32) -> list<search-hit>;
  search-content: func(query: string, limit: u32) -> list<search-hit>;
}
```

permission `read_paths` の範囲内のヒットのみ返す (host で post-filter)。

---

## 3. Plugin exports

### 3.1 commands

```wit
interface commands {
  variant arg-value {
    text(string),
    path(string),
    number(f64),
    bool(bool),
  }

  record command-context {
    selected-paths: list<string>,
    current-dir:    string,
    locale:         string,
  }

  enum command-mode { instant, view, external }

  record command-info {
    id:             string,
    title:          string,
    subtitle:       option<string>,
    icon:           option<string>,
    category:       string,
    mode:           command-mode,
    keywords:       list<string>,
    arguments:      list<arg-spec>,
    default-hotkey: option<string>,
  }

  record arg-spec {
    name:        string,
    kind:        arg-kind,
    placeholder: option<string>,
    required:    bool,
  }
  enum arg-kind { text, path, number, bool, choice }

  variant command-result {
    instant(option<string>),       // 単純実行 (optional ステータステキスト)
    view(view-node),               // launcher 内ビュー表示
    failure(string),               // エラー
  }

  list-commands: func() -> list<command-info>;
  run-command:   func(id: string, args: list<arg-value>, ctx: command-context) -> command-result;
}
```

### 3.2 decorations

```wit
interface decorations {
  record decoration {
    badge:         option<string>,    // 短い badge text (例: "M")
    badge-color:   option<u32>,       // 0xAARRGGBB
    icon-override: option<string>,    // built-in icon id
    text-color:    option<u32>,
  }
  decorate: func(path: string, meta: file-meta) -> option<decoration>;
}
```

### 3.3 previews

```wit
interface previews {
  record text-preview {
    content:  string,
    language: option<string>,    // syntect language id
  }
  variant preview-content {
    text(text-preview),
    image(list<u8>),
    structured(view-node),
  }
  preview: func(path: string, meta: file-meta) -> option<preview-content>;
}
```

### 3.4 events

```wit
interface events {
  record fs-event {
    path: string,
    kind: fs-event-kind,
  }
  enum fs-event-kind { create, modify, delete, rename }

  on-fs-event:       func(events: list<fs-event>);
  on-config-change:  func();
  on-suspend:        func();
  on-resume:         func();
}
```

---

## 4. UI レンダリングモデル — `view-node`

原則: **「データを返させて、描画はホストが行う」**。plugin は構造化データを返し、host が GPUI で描画。

```wit
variant view-node {
  list(list-view),
  detail(detail-view),       // markdown body + metadata sidebar
  form(form-view),           // 入力フィールド集合
  empty(empty-view),         // empty state with message
  loading(loading-view),
}

record list-view {
  items:    list<list-item>,
  sections: list<section-info>,
  empty:    option<string>,
}

record list-item {
  id:          string,
  title:       string,
  subtitle:    option<string>,
  icon:        option<string>,
  accessories: list<accessory>,
  actions:     list<action>,
  badge:       option<badge>,
  kind:        option<string>,
}

record accessory {
  text:    string,
  tooltip: option<string>,
}

record action {
  id:       string,
  title:    string,
  shortcut: option<string>,
  icon:     option<string>,
}

record detail-view {
  markdown:        string,
  metadata-pairs:  list<tuple<string, string>>,   // sidebar key/value
}

record form-view {
  fields:    list<form-field>,
  submit-id: string,
}

record form-field {
  id:           string,
  label:        string,
  kind:         form-field-kind,
  required:     bool,
  default-value: option<string>,
  placeholder:  option<string>,
}
enum form-field-kind { text, password, number, path, choice, multiline }

record empty-view {
  title: string,
  hint:  option<string>,
  icon:  option<string>,        // icon id (§6 参照)
}

record loading-view {
  message: option<string>,
}

record section-info {
  id:       string,
  title:    string,
  subtitle: option<string>,
}

record badge {
  label: string,
  color: option<u32>,           // 0xAARRGGBB (§6 参照)
  kind:  option<string>,        // 任意の意味づけ (例: "status")
}
```

### 4.1 採用範囲

- ✅ 構造化リスト (Raycast 流): 表示要素はすべて型付き
- ✅ markdown (`detail-view.markdown`): 自由文表示
- ❌ element tree (任意の GPUI element の表現): **非採用** (WIT が爆発し、版互換性が壊れやすい)

---

## 5. 通信モデル

| 観点 | 仕様 |
|------|------|
| host → plugin | **sync** (wasmtime sync API)。host 側で `cx.background_spawn` 経由で実行し UI を block しない |
| plugin → host | **sync** (WIT 関数は sync 呼び出し) |
| 長時間処理 | P4 では cancel 不可。plugin 側で chunk 化推奨 |
| async task | P5+ で検討。`host.spawn-task` で投げて結果を `events.on-task-complete` で受け取る形 |

---

## 6. データ型の表現

| 概念 | WIT 表現 | 備考 |
|------|---------|------|
| path | `string` | UTF-8 で正規化、各 OS で適切に変換 |
| binary data | `list<u8>` | |
| timestamp | `u64` (nanoseconds since epoch) | |
| file metadata | `record file-meta` | §2.4 |
| color | `u32` (0xAARRGGBB) | |
| icon | `string` (icon id) — built-in は `"icon:folder"`, plugin 提供は `"plugin:my-plugin/icons/foo.svg"` | |

---

## 7. WIT 配置

```text
crates/nohrs-plugin-host/wit/
├── world.wit
├── deps/
│   └── nohrs-types/        # 共通 record / variant
└── interfaces/             # interface 単位の分割
    ├── logging.wit
    ├── kv.wit
    ├── cache.wit
    ├── ...
```

---

## 8. plugin 作者向けドキュメント

- 各 interface は web の `/docs/plugin-authoring/api/<interface>` で詳細解説 (P4 で公開)
- WIT の生 schema は GitHub raw URL でも参照可
- AI agent 向け MCP server `@nohrs/mcp-plugin-dev` の `nohrs_wit_lookup(name)` で query 可能
