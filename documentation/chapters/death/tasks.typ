#import "../../components/major.typ": major
#import "@preview/cheq:0.3.0": checklist
#show: checklist
#import "../../components/task.typ": task

#major(
  "Tasks",
  $"// TODO"$,
  message: "Don't hallucinate while you implement the following features...",
  by: "Not me",
)

#task(
  title: "Canvas",
  contributors: ("Dave", "Eve"),
)[
  Make an expansive 2d canvas, views dragging to adjust the position.
  When im in any collection, i can drag and drop my views, resize them, and their positions and size it all saved.
  They can overlap and whichever View was meddled with last will have the highest Z index.
  There must be a button to order them to not be stacked, and an undo button.
  The press of the order button makes them fit the screeen as much as possible wrapping downwards
]

#task(
  title: "Final Report",
  contributors: ("Dave", "Eve"),
)[
  This is the *body content*.
  - It supports lists
  - And $#math.arccos$ logic!
]
