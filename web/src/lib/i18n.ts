/* i18n core (web.md §4). Path-prefix routing (/en, /ja), canonical = en,
   full bilingual parity at launch. The `en` tree is the source of shape;
   `ja` must match it (enforced by the `Messages` type below). */

export const locales = ['en', 'ja'] as const
export type Locale = (typeof locales)[number]
export const defaultLocale: Locale = 'en'

export function isLocale(value: string): value is Locale {
  return (locales as ReadonlyArray<string>).includes(value)
}

/** Map a locale to its BCP-47 tag (for <html lang> and hreflang). */
export const htmlLang: Record<Locale, string> = {
  en: 'en',
  ja: 'ja',
}

const en = {
  site: {
    name: 'nohrs',
    tagline: 'A keyboard-first launcher and file explorer, built in Rust.',
  },
  kickers: {
    why: 'Why',
    features: 'Product',
    craft: 'Engineering',
    social: 'Open source',
    roadmap: 'Roadmap',
  },
  nav: {
    features: 'Features',
    docs: 'Docs',
    blog: 'Blog',
    plugins: 'Plugins',
    releases: 'Releases',
    download: 'Download',
    toggleTheme: 'Toggle theme',
    toggleMenu: 'Toggle menu',
    language: 'Language',
  },
  footer: {
    product: 'Product',
    resources: 'Resources',
    project: 'Project',
    social: 'Social',
    roadmap: 'Roadmap',
    about: 'About',
    community: 'Community',
    discussions: 'Discussions',
    privacy: 'Privacy',
    license: 'License',
    faq: 'FAQ',
    builtWith: 'Built in Rust. Open source.',
    rights: 'Released under the MIT license.',
    comingSoon: 'Coming soon',
  },
  hero: {
    badge: 'Pre-alpha · Built in Rust',
    title: 'Launch anything. Find everything.',
    subtitle:
      'nohrs is a keyboard-first launcher and file explorer for macOS — fast, scriptable, and extensible with sandboxed WASM plugins.',
    download: 'Download for macOS',
    viewGithub: 'View on GitHub',
    screenshotAlt:
      'nohrs file explorer showing a sidebar, file list, and preview pane',
    screenshotCaption:
      'The Explorer today. Launcher, plugins, and search previews below are mockups of work in progress.',
  },
  why: {
    heading: 'Why nohrs?',
    subheading: 'Four ideas the rest of the desktop keeps getting wrong.',
    points: [
      {
        title: 'Launcher, first-class',
        body: 'Not a search box bolted onto Finder. The launcher is the front door — open apps, files, and actions without lifting your hands off the keyboard.',
      },
      {
        title: 'Explorer, first-class',
        body: 'A real file explorer with a sidebar, list, and live preview — designed for browsing and acting on files, not just locating them.',
      },
      {
        title: 'Sandboxed WASM plugins',
        body: 'Extend nohrs with plugins compiled to WebAssembly and run under a capability-based permission model. Power without trusting arbitrary native code.',
      },
      {
        title: "Search that isn't Spotlight",
        body: "A content index you control, built in Rust — no opaque daemon, no waiting for a reindex you can't see.",
      },
    ],
  },
  features: {
    heading: 'One tool, four surfaces',
    subheading:
      'The Explorer ships today. The rest are in active development — shown here honestly as previews.',
    available: 'Available',
    coming: 'Coming in v0.x',
    launcherPlaceholder: 'Open file, app, or action…',
    items: [
      {
        name: 'Explorer',
        status: 'available',
        body: 'Sidebar, file list, and preview pane. Keyboard-driven navigation and quick actions.',
      },
      {
        name: 'Launcher',
        status: 'coming',
        body: 'Type to open apps, files, and actions. Fuzzy-matched and instant.',
      },
      {
        name: 'Plugins',
        status: 'coming',
        body: 'Install WASM plugins from a store, each with explicit permission badges.',
      },
      {
        name: 'Search',
        status: 'coming',
        body: 'Full-content search over your files with an index you own.',
      },
    ],
  },
  craft: {
    heading: 'Built in Rust, for the long haul',
    body: "nohrs is written in Rust end to end — the UI, the file engine, the search index, the plugin host. That buys us speed, memory safety, and a foundation we can keep sharpening for years. The accent you see throughout this site is Rust's own tan: a small reminder of what it's made of.",
    points: [
      'Native performance, no Electron',
      'Memory-safe by construction',
      'A plugin host built on the WASM Component Model',
    ],
  },
  social: {
    heading: 'Open from day one',
    subheading:
      "nohrs is pre-alpha, so we won't fake testimonials. Instead, here's the project in the open.",
    stars: 'GitHub stars',
    activity: 'Recent activity',
    contributors: 'Contributors',
    viewGithub: 'Follow along on GitHub',
    proof: [
      { value: 'MIT', label: 'Open-source license' },
      { value: 'Rust', label: 'Built end to end' },
      { value: 'Public', label: 'Roadmap & issues' },
    ],
    makersNoteHeading: "Why we're building nohrs",
    makersNote:
      "Every launcher we tried treated the keyboard as an afterthought and the file system as someone else's problem. We wanted one tool that takes both seriously — fast enough to disappear, open enough to trust, and built on a foundation that lasts. So we're building it in the open, in Rust. This is early, and honest about being early.",
  },
  roadmap: {
    heading: 'Where this is going',
    subheading: 'A transparent roadmap, phase by phase.',
    viewFull: 'See the full roadmap',
    phases: [
      {
        id: 'P1',
        name: 'Web & foundation',
        body: 'Public site, docs, the project in the open.',
      },
      {
        id: 'P2',
        name: 'Explorer polish',
        body: 'Previews, actions, and navigation that feel native.',
      },
      {
        id: 'P3',
        name: 'Launcher & commands',
        body: 'Keyboard-first launching and a command registry.',
      },
      {
        id: 'P4',
        name: 'Plugin host',
        body: 'Sandboxed WASM plugins with a permission model.',
      },
      {
        id: 'P5',
        name: 'Plugin store',
        body: 'Discover and install community plugins.',
      },
      {
        id: 'P6',
        name: 'Search & scale',
        body: 'Full-content search and broader platform support.',
      },
    ],
  },
  community: {
    heading: 'Come build with us',
    subheading: 'Questions, ideas, and contributions are all welcome.',
    discord: 'Join Discord',
    github: 'Star on GitHub',
    discussions: 'Open a discussion',
  },
  finalCta: {
    heading: 'Try nohrs',
    subheading:
      "macOS, pre-alpha. Honest about where it is — and where it's headed.",
    download: 'Download for macOS',
  },
  about: {
    title: 'About nohrs',
    lead: 'A keyboard-first launcher and file explorer, built in Rust and developed in the open.',
    storyHeading: 'The story',
    story: [
      'nohrs started from a simple frustration: the tools we use to launch apps and move through files have barely changed in a decade, and most of them treat the keyboard as a fallback rather than the primary way to work.',
      'We wanted something different — a launcher and a file explorer that are equally first-class, fast enough to get out of your way, and extensible without asking you to trust arbitrary native code. Rust gave us the performance and safety to build that foundation; WebAssembly gave us a way to run plugins safely.',
      "It's early. nohrs is pre-alpha and changing quickly. We're building it in public so you can see exactly where it is — and help shape where it goes.",
    ],
    valuesHeading: 'What we value',
    values: [
      {
        title: 'Keyboard-first',
        body: 'Your hands stay on the keyboard. The mouse is optional, never required.',
      },
      {
        title: 'Honest by default',
        body: "We show what works and what doesn't. No faked demos, no vanity metrics.",
      },
      {
        title: 'Open and extensible',
        body: "Open source, with a plugin system that doesn't make you choose between power and safety.",
      },
      {
        title: 'Built to last',
        body: 'Rust end to end, so the foundation stays solid as the product grows.',
      },
    ],
    makersNoteHeading: 'A note from the maker',
  },
  download: {
    title: 'Download nohrs',
    lead: "nohrs targets macOS first. It's pre-alpha — here's exactly what that means.",
    prealphaHeading: 'Pre-alpha — read this first',
    prealpha:
      'There is no official release yet. nohrs is under active development and not ready for daily use. The most reliable way to try it today is to build from source. When the first release ships, it will appear here and on the Releases page.',
    macHeading: 'macOS',
    macBody:
      'Prebuilt macOS binaries will be published as GitHub release assets. None are available yet — check back, or build from source below.',
    macButton: 'See releases on GitHub',
    sourceHeading: 'Build from source',
    sourceIntro: "You'll need a recent Rust toolchain. Then:",
    sourceButton: 'View the repository',
    reqHeading: 'Requirements',
    requirements: [
      'macOS (Apple Silicon or Intel)',
      'Rust toolchain (stable) to build from source',
      'Roughly 200 MB of disk space for a debug build',
    ],
  },
  notFound: {
    title: 'Page not found',
    body: "That page doesn't exist — or hasn't been built yet.",
    home: 'Go home',
  },
}

export type Messages = typeof en

const ja: Messages = {
  site: {
    name: 'nohrs',
    tagline: 'Rust 製の、キーボード中心のランチャー兼ファイルエクスプローラ。',
  },
  kickers: {
    why: 'なぜ',
    features: 'プロダクト',
    craft: 'エンジニアリング',
    social: 'オープンソース',
    roadmap: 'ロードマップ',
  },
  nav: {
    features: '機能',
    docs: 'ドキュメント',
    blog: 'ブログ',
    plugins: 'プラグイン',
    releases: 'リリース',
    download: 'ダウンロード',
    toggleTheme: 'テーマ切替',
    toggleMenu: 'メニュー切替',
    language: '言語',
  },
  footer: {
    product: 'プロダクト',
    resources: 'リソース',
    project: 'プロジェクト',
    social: 'ソーシャル',
    roadmap: 'ロードマップ',
    about: '概要',
    community: 'コミュニティ',
    discussions: 'ディスカッション',
    privacy: 'プライバシー',
    license: 'ライセンス',
    faq: 'FAQ',
    builtWith: 'Rust 製。オープンソース。',
    rights: 'MIT ライセンスで公開しています。',
    comingSoon: '近日公開',
  },
  hero: {
    badge: 'プレアルファ · Rust 製',
    title: 'すべてを起動。すべてを発見。',
    subtitle:
      'nohrs は macOS 向けの、キーボード中心のランチャー兼ファイルエクスプローラです。高速・スクリプタブルで、サンドボックス化された WASM プラグインで拡張できます。',
    download: 'macOS 版をダウンロード',
    viewGithub: 'GitHub で見る',
    screenshotAlt:
      'サイドバー・ファイル一覧・プレビューを表示する nohrs ファイルエクスプローラ',
    screenshotCaption:
      '現時点の Explorer です。下部のランチャー・プラグイン・検索プレビューは開発中のモックアップです。',
  },
  why: {
    heading: 'なぜ nohrs か',
    subheading: 'デスクトップが取り違え続けてきた 4 つの発想。',
    points: [
      {
        title: 'ランチャーをファーストクラスに',
        body: 'Finder に検索ボックスを後付けしたものではありません。ランチャーこそ入り口です。キーボードから手を離さずに、アプリ・ファイル・アクションを開けます。',
      },
      {
        title: 'エクスプローラをファーストクラスに',
        body: 'サイドバー・一覧・ライブプレビューを備えた本物のファイルエクスプローラ。探すだけでなく、ファイルを閲覧し操作するために設計しています。',
      },
      {
        title: 'サンドボックス化された WASM プラグイン',
        body: 'WebAssembly にコンパイルしたプラグインで nohrs を拡張。ケイパビリティベースの権限モデルの下で動かします。任意のネイティブコードを信頼せずに力を得られます。',
      },
      {
        title: 'Spotlight ではない検索',
        body: 'あなたが制御できる、Rust 製のコンテンツインデックス。不透明なデーモンも、見えない再インデックス待ちもありません。',
      },
    ],
  },
  features: {
    heading: 'ひとつの道具、4 つの面',
    subheading:
      'Explorer は今すぐ使えます。残りは活発に開発中で、ここでは正直にプレビューとして示します。',
    available: '利用可能',
    coming: 'v0.x で登場予定',
    launcherPlaceholder: 'ファイル・アプリ・アクションを開く…',
    items: [
      {
        name: 'Explorer',
        status: 'available',
        body: 'サイドバー・ファイル一覧・プレビュー。キーボード操作とクイックアクション。',
      },
      {
        name: 'Launcher',
        status: 'coming',
        body: '入力してアプリ・ファイル・アクションを開く。あいまい一致で即座に。',
      },
      {
        name: 'Plugins',
        status: 'coming',
        body: 'ストアから WASM プラグインを導入。各プラグインに明示的な権限バッジ。',
      },
      {
        name: 'Search',
        status: 'coming',
        body: 'あなたが所有するインデックスによる、ファイル全文検索。',
      },
    ],
  },
  craft: {
    heading: 'Rust 製、長く使うために',
    body: 'nohrs は UI・ファイルエンジン・検索インデックス・プラグインホストまで、すべて Rust で書かれています。だから速く、メモリ安全で、何年も磨き続けられる土台になります。サイト全体で使われているアクセントは Rust 自身の tan 色。何でできているかの、小さな目印です。',
    points: [
      'ネイティブ性能、Electron 不使用',
      '構造的にメモリ安全',
      'WASM Component Model 上のプラグインホスト',
    ],
  },
  social: {
    heading: '初日からオープン',
    subheading:
      'nohrs はプレアルファなので、推薦の声を捏造することはしません。代わりに、プロジェクトをそのまま公開します。',
    stars: 'GitHub スター',
    activity: '最近の活動',
    contributors: 'コントリビュータ',
    viewGithub: 'GitHub でフォロー',
    proof: [
      { value: 'MIT', label: 'オープンソースライセンス' },
      { value: 'Rust', label: '隅々まで Rust 製' },
      { value: 'Public', label: 'ロードマップと課題' },
    ],
    makersNoteHeading: 'なぜ nohrs を作るのか',
    makersNote:
      '試したランチャーはどれもキーボードを後回しにし、ファイルシステムを他人事のように扱っていました。私たちは、その両方を本気で扱うひとつの道具が欲しかった。消えるほど速く、信頼できるほどオープンで、長持ちする土台の上に。だから Rust で、オープンに作っています。まだ初期です。そして、初期であることに正直です。',
  },
  roadmap: {
    heading: 'これからの方向',
    subheading: 'フェーズごとの、透明なロードマップ。',
    viewFull: 'ロードマップ全体を見る',
    phases: [
      {
        id: 'P1',
        name: 'Web と基盤',
        body: '公開サイト、ドキュメント、オープンな開発。',
      },
      {
        id: 'P2',
        name: 'Explorer の磨き込み',
        body: 'ネイティブに感じるプレビュー・アクション・ナビ。',
      },
      {
        id: 'P3',
        name: 'ランチャーとコマンド',
        body: 'キーボード中心の起動とコマンドレジストリ。',
      },
      {
        id: 'P4',
        name: 'プラグインホスト',
        body: '権限モデルを備えたサンドボックス WASM プラグイン。',
      },
      {
        id: 'P5',
        name: 'プラグインストア',
        body: 'コミュニティのプラグインを発見・導入。',
      },
      {
        id: 'P6',
        name: '検索とスケール',
        body: '全文検索と、より広いプラットフォーム対応。',
      },
    ],
  },
  community: {
    heading: '一緒に作りましょう',
    subheading: '質問・アイデア・コントリビュート、どれも歓迎です。',
    discord: 'Discord に参加',
    github: 'GitHub でスター',
    discussions: 'ディスカッションを開く',
  },
  finalCta: {
    heading: 'nohrs を試す',
    subheading: 'macOS、プレアルファ。今どこにいて、どこへ向かうかに正直に。',
    download: 'macOS 版をダウンロード',
  },
  about: {
    title: 'nohrs について',
    lead: 'Rust 製・オープンに開発している、キーボード中心のランチャー兼ファイルエクスプローラ。',
    storyHeading: '物語',
    story: [
      'nohrs は単純な不満から始まりました。アプリを起動しファイルを行き来する道具は、この 10 年ほとんど変わらず、その多くがキーボードを主役ではなく代替手段として扱っています。',
      '私たちは違うものが欲しかった。ランチャーとファイルエクスプローラが等しくファーストクラスで、邪魔にならないほど速く、任意のネイティブコードを信頼せずに拡張できるもの。Rust はその土台を作る性能と安全性を、WebAssembly はプラグインを安全に動かす手段を与えてくれました。',
      'まだ初期です。nohrs はプレアルファで、急速に変化しています。今どこにいるかをそのまま見てもらい、どこへ向かうかを一緒に形作ってもらうために、公開の場で作っています。',
    ],
    valuesHeading: '大切にしていること',
    values: [
      {
        title: 'キーボード中心',
        body: '手はキーボードの上に。マウスは任意で、必須ではありません。',
      },
      {
        title: '標準で正直に',
        body: '動くものと動かないものを示します。捏造したデモも、見栄えだけの指標もありません。',
      },
      {
        title: 'オープンで拡張可能',
        body: 'オープンソース。力と安全性のどちらかを選ばせないプラグインシステム。',
      },
      {
        title: '長持ちする設計',
        body: '端から端まで Rust。プロダクトが育っても土台は堅牢なまま。',
      },
    ],
    makersNoteHeading: '作り手からの一言',
  },
  download: {
    title: 'nohrs をダウンロード',
    lead: 'nohrs はまず macOS を対象にしています。プレアルファです。その意味を正確にお伝えします。',
    prealphaHeading: 'プレアルファ — 最初にお読みください',
    prealpha:
      '正式リリースはまだありません。nohrs は活発に開発中で、日常利用にはまだ向きません。今試す最も確実な方法はソースからのビルドです。最初のリリースが出れば、ここと Releases ページに掲載します。',
    macHeading: 'macOS',
    macBody:
      'ビルド済み macOS バイナリは GitHub のリリースアセットとして公開予定です。まだ提供はありません。後日ご確認いただくか、下記のソースビルドをお試しください。',
    macButton: 'GitHub のリリースを見る',
    sourceHeading: 'ソースからビルド',
    sourceIntro: '新しめの Rust ツールチェインが必要です。次を実行します:',
    sourceButton: 'リポジトリを見る',
    reqHeading: '動作要件',
    requirements: [
      'macOS（Apple Silicon または Intel）',
      'ソースビルド用の Rust ツールチェイン（stable）',
      'デバッグビルドでおよそ 200 MB のディスク空き容量',
    ],
  },
  notFound: {
    title: 'ページが見つかりません',
    body: 'そのページは存在しないか、まだ作られていません。',
    home: 'ホームへ',
  },
}

export const messages: Record<Locale, Messages> = { en, ja }

export function getMessages(locale: Locale): Messages {
  return messages[locale]
}

/** Swap the locale prefix of a path (e.g. /en/about -> /ja/about). */
export function localizePath(pathname: string, locale: Locale): string {
  const rest = pathname.replace(/^\/(en|ja)(?=\/|$)/, '')
  return `/${locale}${rest}`
}
