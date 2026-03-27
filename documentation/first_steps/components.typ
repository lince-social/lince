#import "../common/typst/components/chapter.typ": major
#import "@preview/touying:0.6.1": *
#import themes.simple: *
#import "@preview/mmdr:0.2.1": mermaid

#let slides-mode = sys.inputs.at("slides", default: "false") == "true"
#let doc-lang = sys.inputs.at("lang", default: "en")
#let doc-region = if doc-lang == "pt" { "br" } else { "us" }
#let book_major = major

#let slides-background = rgb(20, 20, 20)
#let slides-foreground = white

#let slides-deck(title, subtitle: none, body) = [
  #show: simple-theme.with(aspect-ratio: "16-9")
  #set page(fill: slides-background)
  #set text(
    fill: slides-foreground,
    font: "New Computer Modern Math",
    size: 20pt,
    lang: doc-lang,
    region: doc-region,
  )
  #set heading(numbering: "1.")

  #slide[
    #align(center + horizon)[
      #v(-4em)
      #image("../common/media/logo/white.svg", width: 18%)
      #v(-2em)
      #text(size: 52pt, weight: "bold")[#title]
      #if subtitle != none [
        #v(-2em)
        #text(size: 20pt)[#subtitle]
      ]
    ]
  ]

  #slide[
    #text(size: 30pt, weight: "bold")[Outline]
    #v(1.2em)
    #outline(target: heading.where(level: 1))
  ]

  #body
]

#let major(title, fancy, message: "", by: "") = {
  if slides-mode {
    [
      = #title
      #align(center + horizon)[
        #text(size: 28pt)[#fancy]
        #if message != "" and by != "" [
          #v(1em)
          #text(size: 18pt)[#message]
          #v(0.5em)
          #text(size: 15pt)[#by]
        ]
      ]
    ]
  } else {
    [
      #book_major(title, fancy, message: message, by: by)
    ]
  }
}

#let idea(title, visual-text: none, body) = {
  let visual = if visual-text != none {
    mermaid(
      visual-text,
      base-theme: "default",
      theme: (
        font_size: 24.0,
        background: "#f4f4f400",
        primary_text_color: "#ffffff",
        primary_color: "#ffffff00",
        primary_border_color: "#ffffff",
        line_color: "#ffffff",
      ),
      layout: (
        node_spacing: 1,
        rank_spacing: 1,
      ),
    )
  } else {}

  if slides-mode {
    [
      #slide[
        #align(start + top)[
          #text(size: 28pt, weight: "bold")[#title]
        ]
        #align(center + horizon)[
          #block(width: 100%)[#visual]
        ]
      ]
    ]
  } else {
    box()[
      == #title
      #if visual != none [
        #align(center + horizon)[
          #block(visual, width: 50%)
        ]
      ]
      #body
    ]
  }
}
