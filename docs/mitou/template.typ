#let project(
  project_name: "",
  applicants: (),
  body
) = {
  // Set the document's basic properties.
  set document(title: project_name)
  set page(
    paper: "a4",
    margin: (
      top: 72pt,
      bottom: 72pt,
      left: 54pt,
      right: 54pt,
    ),
  )

  // Configure text
  set text(
    font: ("Hiragino Mincho ProN", "YuMincho", "MS Mincho"),
    size: 10.5pt,
    lang: "ja"
  )

  // Configure paragraphs
  set par(
    first-line-indent: 1em,
    justify: true,
    leading: 0.8em
  )

  // Header info
  if project_name != "" {
    text(weight: "bold")[プロジェクト名：#project_name]
    linebreak()
  }
  if applicants.len() > 0 {
    text(weight: "bold")[申請者名：#applicants.join("、")]
    linebreak()
  }

  v(1em)

  // Main Title
  align(center)[
    #text(weight: "bold", size: 12pt)[【提案プロジェクト詳細】]
  ]

  v(2em)

  // Configure Headings (for bullet points in the prompt)
  show heading: it => {
    v(1em)
    text(weight: "bold")[#it.body]
    v(0.5em)
  }

  // Display the body
  body
}
