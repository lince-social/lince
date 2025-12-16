#import "chapters/aging/birth.typ"
#import "aging.typ"
#import "chapters/sickness/sickness.typ"
#import "chapters/death.typ"

#import "@preview/hydra:0.6.2": hydra
#import "@preview/cheq:0.3.0": checklist
#show: checklist

#set document(title: [Instinctus], author: "Eduardo de Melo Xavier")
#set text(
  lang: "en",
  region: "us",
  font: "New Computer Modern Math",
  weight: "regular",
  size: 12pt,
)
#place(
  center + horizon,
)[
  #title(align(center, text(40pt, "Instinctus")))
  #title(align(center, text(15pt, "Lince Documentation")))
]

#pagebreak()

Typst Version: #dictionary(sys).at("version") \ \
Starting Date: #datetime(year: 2025, month: 12, day: 15).display() \ \
Date of Print:  #datetime.today().display() \ \
Days of Construction: #(datetime.today() - datetime(year: 2025, month: 12, day: 15)).days() \ \
Source: Lince (\@lince-social/lince)
#footnote["https://github.com/lince-social/lince"] \

#pagebreak()
#outline()
#pagebreak()

#set page(
  margin: (top: 1in, bottom: 1in, left: 1in, right: 1in),
  footer: align(right, [
    #context here().page() / #context counter(page).final().first()
  ]),
)
#set heading(numbering: "1.")
#let current_major = context hydra(1, skip-starting: false)
#set page(
  header: align(center, [
    #current_major
  ]),
  footer: align(right, [
    #context counter(page).display() / #context counter(page).final().first()
  ]),
)
#show heading: block.with(
  spacing: 2em,
)

#birth
#aging
#sickness
#death

#pagebreak()

#bibliography("chapters/bibliography.yml")
