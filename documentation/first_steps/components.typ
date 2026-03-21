#import "../common/typst/components/chapter.typ": major
#import "@preview/touying:0.6.1": *
#import themes.simple: *

#let slides-mode = sys.inputs.at("slides", default: "false") == "true"
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
  )
  #set heading(numbering: "1.")

  #slide[
    #align(center + horizon)[
      #image("../common/media/logo/white.svg", width: 18%)
      #v(1.2em)
      #text(size: 34pt, weight: "bold")[#title]
      #if subtitle != none [
        #v(0.8em)
        #text(size: 22pt)[#subtitle]
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

#let idea(title, photo: none, body) = {
  if slides-mode {
    [
      #slide[
        #align(center + horizon)[
          #text(size: 28pt, weight: "bold")[#title]
          #if photo != none [
            #image(photo, width: 40%)
          ]
        ]
      ]
    ]
  } else {
    [
      == #title
      #if photo != none [
        #align(center + horizon)[
          #v(-20em)
          #image(photo, width: 50%)
        ]
      ]
      #body
    ]
  }
}
