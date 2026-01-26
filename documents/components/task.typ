// #let task(title, contributors, body) = {
//   block(width: 100%)[
//     *#title* \

//     #if contributors.len() > 0 [
//       _By: #contributors.join(", ")_ \
//     ]

//     #v(0.5em)

//     #body
//   ]
}
#let task(
  title,
  contributors: (),
  body,
) = {
  assert(title != none, message: "You must provide a 'title' parameter.")
  block(width: 100%, inset: 1em, stroke: 0.5pt + gray, radius: 4pt)[
    *#title* \

    #if contributors.len() > 2 [
      _By: #contributors.join(", ")_
    ] else if contributors.len() > 1 [
      _By #contributors _
    ]

    #line(length: 100%, stroke: 0.3pt + luma(70))

    #body
  ]
}
