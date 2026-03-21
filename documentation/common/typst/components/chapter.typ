#let dark-mode = sys.inputs.at("dark", default: "false") == "true"
#let page-background = if dark-mode { rgb("#000000") } else { white }
#let text-color = if dark-mode { rgb("#eaeaea") } else { black }

#let major(title, fancy, message: "", by: "") = {
  pagebreak()
  set page(fill: page-background, header: none, footer: none)
  set text(fill: text-color)

  place(
    center + horizon,
    block(
      align(center)[
        #text(30pt, fill: text-color)[#fancy]
        #text(20pt, fill: text-color)[#heading([#title])]
        #text(16pt, fill: text-color)[
          #if message != "" {
            if by != "" {
              set quote(block: true)
              quote(attribution: [#by])[#message]
            }
          }
        ]
      ],
    ),
  )
  pagebreak()
}
