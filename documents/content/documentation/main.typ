#import "birth/birth.typ"
#import "aging/aging.typ"
#import "sickness/sickness.typ"
#import "death/death.typ"

#import "@preview/hydra:0.6.2": hydra
#import "@preview/cheq:0.3.0": checklist
#show: checklist

// Dark mode configuration via input variable (default: false for light mode)
#let dark-mode = sys.inputs.at("dark", default: "false") == "true"

#let bg-color = if dark-mode { rgb("#000000") } else { white }
#let text-color = if dark-mode { rgb("#eaeaea") } else { black }
#let link-color = if dark-mode { rgb("#6ea8fe") } else { rgb("#0000EE") }
#let logo-path = if dark-mode { "../../media/logo/branco.png" } else {
  "../../media/logo/preto.png"
}

#set page(fill: bg-color)
#set document(title: [Instinto], author: "Eduardo de Melo Xavier")
#set text(
  lang: "en",
  region: "us",
  // font: "New Computer Modern Math",
  weight: "regular",
  size: 12pt,
  fill: text-color,
)
#place(
  center + horizon,
)[
  #figure(image(logo-path, width: 40%))
  #title(align(center, text(65pt, "Instinto")))
  #title(align(center, text(20pt, "Lince Documentation")))
]

#show link: it => underline(text(fill: link-color, it))
#pagebreak()
Lince Version: 0.6.1 \ \
Typst Version: #dictionary(sys).at("version") \ \
Documentation Start: #datetime(year: 2025, month: 12, day: 15).display() \ \
Documentation Print:  #datetime.today().display() \ \
Days of Construction: #(datetime.today() - datetime(year: 2025, month: 12, day: 15)).days() \ \
Source: #link("https://github.com/lince-social/lince")[Lince] (\@lince-social/lince)
#footnote["https://github.com/lince-social/lince"] \

#pagebreak()
#outline()

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

// #pagebreak()

// #bibliography("bibliography.yml")
