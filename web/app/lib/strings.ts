/**
 * Every string the chrome and the built pages need, in both languages.
 *
 * `ja` is typed as `typeof en`, so a key added to one language and forgotten in
 * the other is a type error rather than an English word on a Japanese page —
 * docs/web.md §4 requires full parity at launch, and nothing else enforces it.
 *
 * Long-form copy (blog posts, docs pages) lives in `content/<lang>/`, not here.
 */

const en = {
  nav: {
    features: 'Features',
    docs: 'Docs',
    blog: 'Blog',
    plugins: 'Plugins',
    releases: 'Releases',
    roadmap: 'Roadmap',
    about: 'About',
    download: 'Download',
    skipToContent: 'Skip to content',
    language: 'Language',
    theme: 'Theme',
    light: 'Light theme',
    dark: 'Dark theme',
    star: 'Star on GitHub',
    joinDiscord: 'Join Discord',
    menu: 'Menu',
  },

  hero: {
    sub: 'Nohrs is a keyboard-driven file explorer for macOS, with a launcher built into the same application. Written in Rust, open source under MIT.',
    tryTitle: 'Build Nohrs from source on macOS — one clone and one command.',
    tryTitleReleased: 'Builds are published with every release. You can also build from source on macOS.',
    screens: 'Screens',
    hint: 'Press any screen to see it in full',
    close: 'Close',
    shots: {
      explorer: {
        label: 'Explorer',
        caption:
          'The sidebar, the listing and the preview pane in one window — and the whole thing moves under the keyboard.',
      },
      preview: {
        label: 'Preview',
        caption:
          'Markdown, code and text render in the pane beside the listing as soon as the selection lands on them.',
      },
      search: {
        label: 'Search',
        caption:
          'Search from where you are standing: scope it to this folder or the whole tree, and match on the name or the contents.',
      },
      matches: {
        label: 'Matches',
        caption:
          'Hits in file contents come back as a list of paths, with the number of matches found inside each one.',
      },
      source: {
        label: 'Source',
        caption:
          'Open a hit and the file appears next to the results, with the matching line marked where it sits.',
      },
    },
  },

  note: {
    eyebrow: "Maker's note",
    title: 'Looking for a file is not work.',
    body: [
      'On a stock Mac, reaching a file means opening a window, walking down a hierarchy, then switching to a search field when that fails. Each step is small. Together they are most of the time you spend on files.',
      'Nohrs puts a launcher and an explorer in one application to remove those steps. Summon it with a global hotkey, move to where you are going with the keyboard, and preview or act on the file in the same window. It does that, and nothing else — quickly and reliably.',
      'Rust and gpui were chosen so that “quickly and reliably” never has to be traded away. The whole thing is built in the open: the decisions, the arguments that lost, and the work still ahead of it.',
    ],
  },

  why: {
    eyebrow: 'Why Nohrs',
    title: 'Four things it refuses to compromise on.',
    pillars: [
      {
        title: 'A launcher, not an afterthought',
        body: 'A built-in launcher you summon from a global hotkey — designed with the explorer, not added to it later.',
      },
      {
        title: 'Everything a modern file manager owes you',
        body: 'Split view, tabs, drag-and-drop and bulk operations — the baseline, done properly.',
      },
      {
        title: 'Sandboxed WASM components',
        body: 'Extend Nohrs in Rust, TypeScript or Python. Every plugin runs sandboxed under an explicit-consent permission model.',
      },
      {
        title: 'No dependency on the OS index',
        body: 'A self-contained SQLite + Tantivy hybrid index that understands code bases — it does not wait on the system search daemon.',
      },
    ],
  },

  preview: {
    eyebrow: 'The application',
    title: 'One window, from finding a file to acting on it.',
    lede: 'The sidebar, the listing and the preview pane are one surface. Nothing here opens a second window.',
    caption: 'Explorer — list and grid views, with the preview pane open',
    parts: [
      {
        index: '01',
        title: 'Move with the keyboard',
        body: 'Up and down the tree, between list and grid, across a selection of many files — each of them has a key. The pointer keeps working for the times you want it.',
      },
      {
        index: '02',
        title: 'Read a file without opening it',
        body: 'Markdown, code and text render in the pane beside the listing as soon as the selection lands on them.',
      },
      {
        index: '03',
        title: 'Act on files where they are',
        body: 'Copy, move, rename, create, trash and delete — across volumes, and without leaving the window.',
      },
    ],
  },

  roadmap: {
    eyebrow: 'Roadmap',
    title: 'Six phases, shipped in order.',
    lede: 'Where the project goes next, and the order it gets there in.',
    pageTitle: 'Roadmap',
    pageLede:
      'Nohrs runs six serial phases from 0.0.x to 0.5.0, and stabilises into 1.0.0 after that. Within a phase, core, web and quality work run in parallel. This page mirrors ROADMAP.md in the repository.',
    versioningTitle: 'How versions are cut',
    versioning: [
      'Before 0.1.0 (P1, pre-MVP), only the patch digit moves: 0.0.z. Nothing is promised about stability, and breaking changes land whenever they need to.',
      '0.1.0 is cut the moment P2 completes — drag-and-drop, file operations, split view, tabs and persistence, the first explorer you can actually use.',
      'Each later phase completes into its own minor bump, and every breaking change to the config schema, the on-disk format or the plugin WIT API is saved for that moment.',
      '1.0.0 promises a stable public API, config and data format. It waits on the multi-OS decision and complete documentation.',
    ],
    states: { shipped: 'Shipped', inProgress: 'In progress', planned: 'Planned' },
    phases: [
      {
        id: 'P1',
        version: '0.0.x',
        theme: 'Foundation — quality, workspace split, dev/CI infrastructure, web',
        state: 'inProgress' as const,
      },
      {
        id: 'P2',
        version: '0.1.0',
        theme: 'Explorer essentials — drag-and-drop, file operations, split view, tabs, persistence',
        state: 'planned' as const,
      },
      {
        id: 'P3',
        version: '0.2.0',
        theme: 'Launcher & search — global-hotkey launcher, SQLite FTS5 search',
        state: 'planned' as const,
      },
      {
        id: 'P4',
        version: '0.3.0',
        theme: 'Plugin host — WASM component model, templates for three languages',
        state: 'planned' as const,
      },
      {
        id: 'P5',
        version: '0.4.0',
        theme: 'Ecosystem — plugin store, community plugins',
        state: 'planned' as const,
      },
      {
        id: 'P6',
        version: '0.5.0',
        theme: 'Stabilization — multi-OS strategy, performance gates, documentation',
        state: 'planned' as const,
      },
    ],
  },

  openSource: {
    eyebrow: 'Open source',
    title: 'All of it is in the repository.',
    lede: 'Every commit, every issue and every decision that shaped Nohrs is public.',
    stars: 'Stars',
    forks: 'Forks',
    issues: 'Open issues',
    license: 'License',
    recent: 'Recent commits',
    fetched: 'Read from GitHub on',
  },

  community: {
    eyebrow: 'Community',
    title: 'Come build it.',
    github: 'Source, issues and pull requests — noh-rs/nohrs',
    discord: 'Design discussion and questions',
    x: 'Release notes and development updates — @nohdotrs',
  },

  about: {
    title: 'About Nohrs',
    lede: 'A file manager for people who would rather not think about the file manager.',
    valuesTitle: 'What the project holds to',
    values: [
      {
        title: 'Say what is built, and what is not',
        body: 'Screenshots are of the application as it actually builds, every roadmap phase carries its status, and nothing on this site is a mockup of a feature that does not exist.',
      },
      {
        title: 'The keyboard is the primary interface',
        body: 'Anything you can do with the pointer should have a key that does it faster. The pointer stays supported; it is not what the design is measured against.',
      },
      {
        title: 'Extensions cannot be asked to be trustworthy',
        body: 'Plugins are WASM components in a sandbox, and every capability they hold is one you granted explicitly. Trust is a property of the host, not of the plugin author.',
      },
      {
        title: 'Everything happens in public',
        body: 'Design decisions land as ADRs in the repository before the code does. The arguments that were rejected stay written down next to the one that won.',
      },
    ],
    stackTitle: 'What it is made of',
    stack: [
      { label: 'Language', body: 'Rust, across a Cargo workspace of layered crates' },
      { label: 'UI', body: 'gpui — the GPU-accelerated framework Zed is built on' },
      { label: 'Search', body: 'SQLite + Tantivy, self-contained, no Spotlight dependency' },
      { label: 'Plugins', body: 'WASM Component Model, via wit-bindgen' },
      { label: 'Platform', body: 'macOS. Whether to go further is a decision for a later phase' },
      { label: 'License', body: 'MIT' },
    ],
  },

  download: {
    title: 'Download',
    ledePreRelease:
      'Nohrs is built from source on macOS — one clone and one command. Binaries arrive with the first release.',
    ledeReleased: 'macOS builds are published with each release.',
    buildTitle: 'Build from source',
    buildIntro: 'You need a Rust toolchain and Xcode command line tools. The build takes a few minutes from cold.',
    requirementsTitle: 'Requirements',
    requirements: [
      { label: 'OS', body: 'macOS 13 or later, Apple silicon or Intel' },
      { label: 'Rust', body: 'The toolchain pinned in rust-toolchain.toml, installed by rustup' },
      { label: 'Xcode', body: 'Command line tools (xcode-select --install)' },
    ],
    watchTitle: 'Get told when there is something to download',
    watchBody:
      'Watch releases on GitHub and you hear first. Release notes also go out on X.',
    watchCta: 'Watch releases on GitHub',
  },

  releases: {
    title: 'Releases',
    lede: 'Every published build, newest first.',
    empty:
      'No releases yet. Each one arrives here with its notes and its macOS build, newest first.',
    emptyCta: 'Follow the roadmap',
    viewOnGitHub: 'Read the release notes on GitHub',
    downloads: 'Downloads',
    prerelease: 'Pre-release',
  },

  blog: {
    title: 'Blog',
    lede: 'Release announcements and notes on how Nohrs is built.',
    empty: 'No posts yet.',
    readingSuffix: 'min read',
    backToIndex: 'All posts',
    publishedOn: 'Published',
    tags: 'Tags',
    commentsTitle: 'Comments',
    commentsBody: 'Replies go to this article’s thread on GitHub.',
    commentsLoading: 'Reading the thread…',
    commentsEmpty: 'Nothing has been said about this article yet.',
    commentsTruncated: 'This thread is longer than what is shown here. The rest is on GitHub.',
    commentsReply: 'Reply on GitHub',
    commentsStart: 'Start the thread on GitHub',
    commentsError: 'The thread could not be loaded. It is readable on GitHub.',
    commentsDeleted: 'Deleted account',
    commentsReactions: 'Reactions to this article',
    translationMissingTitle: 'This article has no Japanese translation yet',
    translationMissingBody: 'The English text is shown below. A translation pull request is very welcome.',
  },

  docs: {
    title: 'Documentation',
    lede: 'Installing Nohrs, using it, and writing plugins for it.',
    searchLabel: 'Search the documentation',
    searchPlaceholder: 'Search docs',
    searchShort: 'Search',
    searchHint: 'Press / to search',
    searchEmpty: 'Nothing matched.',
    onThisPage: 'On this page',
    categories: {
      'getting-started': 'Getting started',
      usage: 'Usage',
      plugins: 'Plugin authoring',
      reference: 'Reference',
    } as Record<string, string>,
    editOnGitHub: 'Edit this page on GitHub',
    previous: 'Previous',
    next: 'Next',
    translationMissingTitle: 'This page has no Japanese translation yet',
    translationMissingBody: 'The English text is shown below. A translation pull request is very welcome.',
  },

  plugins: {
    title: 'Plugins',
    lede: 'Nohrs plugins are WASM components. They run in a sandbox and hold only the capabilities you grant them.',
    previewNotice:
      'Every plugin here lists the permissions it asks for. Nohrs hands over nothing that is not on that list.',
    categories: {
      productivity: 'Productivity',
      'developer-tools': 'Developer tools',
      media: 'Media',
      cloud: 'Cloud',
      theme: 'Themes',
    } as Record<string, string>,
    allCategories: 'All',
    author: 'Author',
    permissions: 'Permissions',
    install: 'Install',
    installHint:
      'Nohrs installs from this address once the plugin host arrives in P4. The permissions above are asked for before anything is installed.',
    source: 'Source',
    submitTitle: 'Adding a plugin',
    submitBody:
      'The registry is a directory of TOML files in this repository. Adding a plugin is a pull request that adds one file; the build then reads the rest from its GitHub repository.',
    empty: 'No plugins are listed yet.',
  },

  footer: {
    tagline: 'A launcher and file explorer for macOS, built in Rust.',
    product: 'Product',
    resources: 'Resources',
    project: 'Project',
    social: 'Social',
    preview: 'Preview',
    discussions: 'Discussions',
    makersNote: "Maker's note",
    contributing: 'Contributing',
    license: 'License',
    rights: '© 2026 Nohrs',
    builtWith: 'Built in Rust. Site source lives in the same repository.',
  },

  notFound: {
    title: 'That page does not exist',
    body: 'The link may be from an older version of the site, or it may be a typo.',
    cta: 'Go to the home page',
  },
}

export type Dict = typeof en

const ja: Dict = {
  nav: {
    features: '機能',
    docs: 'ドキュメント',
    blog: 'ブログ',
    plugins: 'プラグイン',
    releases: 'リリース',
    roadmap: 'ロードマップ',
    about: 'プロジェクトについて',
    download: 'ダウンロード',
    skipToContent: '本文へスキップ',
    language: '言語',
    theme: 'テーマ',
    light: 'ライトテーマ',
    dark: 'ダークテーマ',
    star: 'GitHub でスターする',
    joinDiscord: 'Discord に参加',
    menu: 'メニュー',
  },

  hero: {
    sub: 'Nohrs は、ランチャーを同じアプリに内蔵した、macOS 向けのキーボード操作中心のファイルエクスプローラーです。Rust 製、MIT ライセンスのオープンソースです。',
    tryTitle: 'macOS でソースからビルドします。clone とコマンド 1 つです。',
    tryTitleReleased:
      'ビルド済みのバイナリは、リリースごとに配布しています。macOS 上でソースからビルドすることもできます。',
    screens: '画面',
    hint: 'まわりの画面は、押すと大きく開きます',
    close: '閉じる',
    shots: {
      explorer: {
        label: 'エクスプローラー',
        caption:
          'サイドバー・一覧・プレビューを 1 つのウィンドウにまとめています。移動はすべてキーボードで完結します。',
      },
      preview: {
        label: 'プレビュー',
        caption:
          '選択したファイルは、一覧の隣のペインにそのまま表示されます。Markdown もコードもテキストも、別のアプリを開かずに読めます。',
      },
      search: {
        label: '検索',
        caption:
          'いま開いている場所から検索します。このフォルダだけか全体か、ファイル名か中身か、を選べます。',
      },
      matches: {
        label: 'マッチ',
        caption: '中身のヒットはパスの一覧で返ります。ファイルごとの一致件数が付きます。',
      },
      source: {
        label: 'ソース',
        caption: 'ヒットを開くと、結果の隣にファイルが並び、一致した行に印が付きます。',
      },
    },
  },

  note: {
    eyebrow: 'メーカーズノート',
    title: 'ファイルを探している時間は、仕事ではありません。',
    body: [
      '標準の macOS では、目的のファイルにたどり着くまでに、ウィンドウを開き、階層をたどり、行き詰まったら検索窓に切り替える、という手数がかかります。一つひとつは小さな操作ですが、合計するとファイルに費やす時間のほとんどがそれです。',
      'Nohrs は、ランチャーとエクスプローラーを 1 つのアプリにまとめることで、この手数をなくします。グローバルホットキーで呼び出し、キーボードだけで目的地まで移動し、同じウィンドウでプレビューして操作する。やるのはそれだけで、それを速く、確実にやります。',
      'Rust と gpui を選んだのは、その「速く、確実に」を妥協せずに済むからです。設計の判断も、採用しなかった案も、これから作るものも、すべて公開の場に置いています。',
    ],
  },

  why: {
    eyebrow: 'Nohrs の四本柱',
    title: '妥協しない四つのこと。',
    pillars: [
      {
        title: '後付けではない、ランチャー',
        body: 'グローバルホットキーで呼び出せる内蔵ランチャー。後から足したものではなく、エクスプローラーと一緒に設計しています。',
      },
      {
        title: '現代のファイルマネージャに求められること',
        body: '分割ビュー、タブ、ドラッグ＆ドロップ、一括操作。当たり前のことを、きちんと。',
      },
      {
        title: 'サンドボックス化された WASM コンポーネント',
        body: 'Rust・TypeScript・Python で拡張できます。すべてのプラグインは、明示的な許可に基づくサンドボックスの中で動きます。',
      },
      {
        title: 'OS の検索インデックスに依存しない',
        body: 'SQLite と Tantivy によるハイブリッドインデックスを自前で持ちます。コードベースを理解し、OS の検索デーモンを待ちません。',
      },
    ],
  },

  preview: {
    eyebrow: 'アプリケーション',
    title: '見つけるところから操作まで、1 つのウィンドウで。',
    lede: 'サイドバーも一覧もプレビューも、1 つのウィンドウの中にあります。ここで別のウィンドウが開くことはありません。',
    caption: 'エクスプローラー — リスト／グリッド表示と、開いたプレビューペイン',
    parts: [
      {
        index: '01',
        title: 'キーボードで動かす',
        body: '階層の上下も、リストとグリッドの切り替えも、複数のファイルにまたがる選択も、それぞれに打鍵があります。ポインタも、使いたいときのために残してあります。',
      },
      {
        index: '02',
        title: '開かずに中身を読む',
        body: 'Markdown もコードもテキストも、選択が乗った時点で、一覧の隣のペインに表示されます。',
      },
      {
        index: '03',
        title: 'その場でファイルを操作する',
        body: 'コピー・移動・リネーム・作成・ゴミ箱・削除。ボリュームをまたぐ場合も、ウィンドウを離れずに済みます。',
      },
    ],
  },

  roadmap: {
    eyebrow: 'ロードマップ',
    title: '六つのフェーズを、順番に。',
    lede: 'これから何を、どの順番で作るか。',
    pageTitle: 'ロードマップ',
    pageLede:
      'Nohrs は 0.0.x から 0.5.0 までを 6 つの直列フェーズで進め、その後 1.0.0 に向けて安定化します。各フェーズの中では、コア・web・品質の 3 つを並行して進めます。このページはリポジトリの ROADMAP.md と対応しています。',
    versioningTitle: 'バージョンの刻み方',
    versioning: [
      '0.1.0 より前 (P1 = pre-MVP) は 0.0.z のみを動かします。安定性の約束はなく、破壊的変更も必要になった時点で入れます。',
      '0.1.0 は P2 の完了と同時に切ります。ドラッグ＆ドロップ、ファイル操作、分割ビュー、タブ、永続化が揃った、最初に実用になるエクスプローラーです。',
      '以降はフェーズの完了ごとにマイナーを上げ、config スキーマ・ディスク上の形式・プラグイン WIT API の破壊的変更は、すべてそのタイミングに集約します。',
      '1.0.0 は、公開 API・config・データ形式の安定を約束するものです。マルチ OS 戦略の決定とドキュメントの完成を待って切ります。',
    ],
    states: { shipped: '完了', inProgress: '進行中', planned: '計画' },
    phases: [
      {
        id: 'P1',
        version: '0.0.x',
        theme: '基盤 — 品質、ワークスペース分割、開発／CI 基盤、web',
        state: 'inProgress' as const,
      },
      {
        id: 'P2',
        version: '0.1.0',
        theme: 'エクスプローラーの基礎 — ドラッグ＆ドロップ、ファイル操作、分割ビュー、タブ、永続化',
        state: 'planned' as const,
      },
      {
        id: 'P3',
        version: '0.2.0',
        theme: 'ランチャーと検索 — グローバルホットキーのランチャー、SQLite FTS5 検索',
        state: 'planned' as const,
      },
      {
        id: 'P4',
        version: '0.3.0',
        theme: 'プラグインホスト — WASM コンポーネントモデル、3 言語のテンプレート',
        state: 'planned' as const,
      },
      {
        id: 'P5',
        version: '0.4.0',
        theme: 'エコシステム — プラグインストア、コミュニティプラグイン',
        state: 'planned' as const,
      },
      {
        id: 'P6',
        version: '0.5.0',
        theme: '安定化 — マルチ OS 戦略、パフォーマンス基準、ドキュメント',
        state: 'planned' as const,
      },
    ],
  },

  openSource: {
    eyebrow: 'オープンソース',
    title: 'すべて、リポジトリにあります。',
    lede: 'Nohrs を形づくったコミットも、議論も、決定も、すべて公開されています。',
    stars: 'スター',
    forks: 'フォーク',
    issues: 'オープンな課題',
    license: 'ライセンス',
    recent: '最近のコミット',
    fetched: 'GitHub から取得',
  },

  community: {
    eyebrow: 'コミュニティ',
    title: '一緒に作りませんか。',
    github: 'ソース・課題・プルリクエスト — noh-rs/nohrs',
    discord: '設計の議論と質問',
    x: 'リリース情報と開発の様子 — @nohdotrs',
  },

  about: {
    title: 'Nohrs について',
    lede: 'ファイルマネージャのことを、なるべく考えずに済ませたい人のためのファイルマネージャです。',
    valuesTitle: 'このプロジェクトが守ること',
    values: [
      {
        title: 'できていることと、できていないことを書く',
        body: 'スクリーンショットは実際にビルドできるアプリのものだけを載せ、ロードマップの各フェーズには状態を添えます。存在しない機能のモックは、このサイトのどこにもありません。',
      },
      {
        title: 'キーボードを主にする',
        body: 'ポインタでできることには、それより速い打鍵があるべきだと考えています。ポインタ操作は引き続き使えますが、設計の基準はキーボードに置きます。',
      },
      {
        title: '拡張に「信用してくれ」と言わせない',
        body: 'プラグインはサンドボックス内の WASM コンポーネントで、権限は必ず明示的に許可したものだけを持ちます。信頼はホスト側の性質であって、作者の人柄ではありません。',
      },
      {
        title: 'すべて公開の場で進める',
        body: '設計上の判断は、コードより先に ADR としてリポジトリに入ります。採用しなかった案も、採用した案の隣に残します。',
      },
    ],
    stackTitle: '何でできているか',
    stack: [
      { label: '言語', body: 'Rust。責務ごとに分けた Cargo ワークスペース' },
      { label: 'UI', body: 'gpui — Zed が使っている GPU アクセラレーテッドな UI フレームワーク' },
      { label: '検索', body: 'SQLite + Tantivy。自前で完結し、Spotlight に依存しません' },
      { label: 'プラグイン', body: 'WASM コンポーネントモデル (wit-bindgen)' },
      { label: '対応 OS', body: 'macOS。その先に広げるかどうかは、後のフェーズで決めます' },
      { label: 'ライセンス', body: 'MIT' },
    ],
  },

  download: {
    title: 'ダウンロード',
    ledePreRelease:
      'いまはソースからビルドします。macOS で、clone とコマンド 1 つです。バイナリは最初のリリースから配ります。',
    ledeReleased: 'macOS 向けのビルドを、リリースごとに配布しています。',
    buildTitle: 'ソースからビルドする',
    buildIntro:
      'Rust ツールチェーンと Xcode のコマンドラインツールが必要です。初回のビルドには数分かかります。',
    requirementsTitle: '必要なもの',
    requirements: [
      { label: 'OS', body: 'macOS 13 以降。Apple シリコン / Intel のどちらでも動きます' },
      { label: 'Rust', body: 'rustup で入れる、rust-toolchain.toml に固定したツールチェーン' },
      { label: 'Xcode', body: 'コマンドラインツール (xcode-select --install)' },
    ],
    watchTitle: 'ダウンロードできるようになったら知る',
    watchBody:
      'GitHub でリリースを watch しておくのが確実です。リリース情報は X にも流します。',
    watchCta: 'GitHub でリリースを watch する',
  },

  releases: {
    title: 'リリース',
    lede: '公開したビルドの一覧です。新しいものから並べています。',
    empty:
      'まだリリースはありません。リリースノートと macOS 向けビルドを添えて、新しいものから順にここに並びます。',
    emptyCta: 'ロードマップを見る',
    viewOnGitHub: 'GitHub でリリースノートを読む',
    downloads: 'ダウンロード',
    prerelease: 'プレリリース',
  },

  blog: {
    title: 'ブログ',
    lede: 'リリースの告知と、Nohrs をどう作っているかの記録です。',
    empty: 'まだ記事がありません。',
    readingSuffix: '分で読めます',
    backToIndex: '記事一覧',
    publishedOn: '公開日',
    tags: 'タグ',
    commentsTitle: 'コメント',
    commentsBody: '返信は、この記事の GitHub のスレッドに書けます。',
    commentsLoading: 'スレッドを読み込んでいます…',
    commentsEmpty: 'この記事について、まだ何も書かれていません。',
    commentsTruncated: 'このスレッドには、ここに出ている以上の書き込みがあります。続きは GitHub にあります。',
    commentsReply: 'GitHub で返信する',
    commentsStart: 'GitHub で最初に書く',
    commentsError: 'スレッドを読み込めませんでした。GitHub では読めます。',
    commentsDeleted: '削除されたアカウント',
    commentsReactions: 'この記事へのリアクション',
    translationMissingTitle: 'この記事の日本語訳は、まだありません',
    translationMissingBody: '以下は英語の本文です。翻訳のプルリクエストを歓迎します。',
  },

  docs: {
    title: 'ドキュメント',
    lede: 'Nohrs の導入・使い方・プラグインの書き方をまとめています。',
    searchLabel: 'ドキュメントを検索',
    searchPlaceholder: 'ドキュメントを検索',
    searchShort: '検索',
    searchHint: '/ で検索',
    searchEmpty: '一致するものがありません。',
    onThisPage: 'このページの見出し',
    categories: {
      'getting-started': 'はじめに',
      usage: '使い方',
      plugins: 'プラグインを書く',
      reference: 'リファレンス',
    },
    editOnGitHub: 'GitHub でこのページを編集する',
    previous: '前のページ',
    next: '次のページ',
    translationMissingTitle: 'このページの日本語訳は、まだありません',
    translationMissingBody: '以下は英語の本文です。翻訳のプルリクエストを歓迎します。',
  },

  plugins: {
    title: 'プラグイン',
    lede: 'Nohrs のプラグインは WASM コンポーネントです。サンドボックスの中で動き、許可した権限だけを持ちます。',
    previewNotice:
      'どのプラグインにも、要求する権限が並んでいます。Nohrs がそこに無いものを渡すことはありません。',
    categories: {
      productivity: '生産性',
      'developer-tools': '開発者向け',
      media: 'メディア',
      cloud: 'クラウド',
      theme: 'テーマ',
    },
    allCategories: 'すべて',
    author: '作者',
    permissions: '権限',
    install: 'インストール',
    installHint:
      'プラグインホストが P4 で入ると、Nohrs はこのアドレスからインストールします。上に並ぶ権限は、その前に確認を求めます。',
    source: 'ソース',
    submitTitle: 'プラグインを追加する',
    submitBody:
      'レジストリは、このリポジトリ内の TOML ファイル群です。追加は 1 ファイルを足すプルリクエストで、残りの情報はビルド時に GitHub リポジトリから読み取ります。',
    empty: 'まだ登録されているプラグインはありません。',
  },

  footer: {
    tagline: 'Rust で作る、macOS 向けのランチャー兼ファイルエクスプローラー。',
    product: 'プロダクト',
    resources: 'リソース',
    project: 'プロジェクト',
    social: 'ソーシャル',
    preview: 'プレビュー',
    discussions: 'ディスカッション',
    makersNote: 'メーカーズノート',
    contributing: 'コントリビュート',
    license: 'ライセンス',
    rights: '© 2026 Nohrs',
    builtWith: 'Rust で作っています。このサイトのソースも同じリポジトリにあります。',
  },

  notFound: {
    title: 'そのページはありません',
    body: '古いバージョンのサイトのリンクか、URL の打ち間違いかもしれません。',
    cta: 'トップページへ',
  },
}

export const strings = { en, ja }
