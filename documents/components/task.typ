#let task(
  title,
  contributors: (),
  body,
) = {
  assert(title != none, message: "You must provide a 'title' parameter.")
  block(width: 100%, inset: 1em, stroke: 0.5pt + gray, radius: 4pt)[
    *#title* \

    _Contributors: #contributors.join(", ")_

    #line(length: 100%, stroke: 0.3pt + luma(70))

    #body
  ]
}
