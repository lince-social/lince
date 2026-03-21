#import "../common/typst/document.typ": book
#import "components.typ": slides-mode, slides-deck
#import "chapters/getting_started.typ"

#if slides-mode [
  #slides-deck([First Steps], subtitle: [Newbies' Tutorial])[
    #getting_started
  ]
] else [
  #book(
    title: "First Steps",
    subtitle: "Newbies' Tutorial",
    start-date: datetime(year: 2026, month: 3, day: 21),
    [
      #getting_started
    ],
  )
]
