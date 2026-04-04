#import "chapters/birth/birth.typ"
#import "chapters/aging/aging.typ"
#import "chapters/sickness/sickness.typ"
#import "chapters/death/death.typ"
#import "chapters/first_steps/first_steps.typ"
#import "components/document.typ": book

#book(
  title: "Instinto",
  subtitle: "Lince's Technical Documentation",
  start-date: datetime(year: 2025, month: 12, day: 15),
  [
    #birth
    #first_steps
    #aging
    #sickness
    #death
  ],
)

// #pagebreak()

// #bibliography("bibliography.yml")
