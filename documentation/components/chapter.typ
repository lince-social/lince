#let major(title, fancy, message: "", by: "") = {
  pagebreak()
  set page(header: none, footer: none)

  place(
    center + horizon,
    block(
      align(center)[
        #text(30pt)[#fancy]
        #text(20pt)[#heading([#title])]
        #text(16pt)[
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
