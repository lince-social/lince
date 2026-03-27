#import "@preview/hydra:0.6.2": hydra
#import "@preview/cheq:0.3.0": checklist

#let dark-mode = sys.inputs.at("dark", default: "false") == "true"
#let lince-version = sys.inputs.at("lince_version", default: "unknown")
#let doc-lang = sys.inputs.at("lang", default: "en")
#let doc-region = if doc-lang == "pt" { "br" } else { "us" }

#let page-background = if dark-mode { rgb("#000000") } else { white }
#let text-color = if dark-mode { rgb("#eaeaea") } else { black }
#let link-color = if dark-mode { rgb("#6ea8fe") } else { rgb("#0000EE") }

#let footer-content(left, right: none) = [
  #block(width: 100%)[
    #text(fill: text-color, size: 10pt)[#left]
    #if right != none [
      #h(1fr)
      #text(fill: text-color, size: 10pt)[#right]
    ]
  ]
]

#let outline-page-footer() = none

#let info-page-footer(source_url) = footer-content(
  source_url,
  right: [#context here().page() / #context counter(page).final().first()],
)

#let body-page-footer() = footer-content(
  [#context counter(page).display() / #context counter(page).final().first()],
)

#let body-page-header(current-major) = [
  #block(width: 100%)[
    #h(1fr)
    #text(fill: text-color, size: 10pt)[#current-major]
    #h(1fr)
  ]
]

#let frontmatter(
  title: "Instinto",
  subtitle: "",
  start-date: datetime(year: 2025, month: 12, day: 15),
  author: "Eduardo de Melo Xavier",
  source_url: "https://github.com/lince-social/lince",
  source_label: "Lince",
  logo_dark: "../media/logo/white.svg",
  logo_light: "../media/logo/black.svg",
) = [
  #show: checklist
  #set page(fill: page-background)
  #set document(title: [#title], author: author)
  #set text(
    lang: doc-lang,
    region: doc-region,
    weight: "regular",
    size: 12pt,
    fill: text-color,
  )
  #show link: it => underline(text(fill: link-color, it))

  #let logo-path = if dark-mode { logo_dark } else { logo_light }

  #place(
    center + horizon,
  )[
    #align(center)[
      #stack(
        dir: ttb,
        spacing: 2em,
        image(logo-path, width: 40%),
        text(65pt, title),
        text(20pt, subtitle),
      )
    ]
  ]

  #pagebreak()
  #set page(
    fill: page-background,
    footer: info-page-footer(source_url),
  )
  Lince Version: #lince-version \ \
  Typst Version: #dictionary(sys).at("version") \ \
  Documentation Start: #start-date.display() \ \
  Documentation Print: #datetime.today().display() \ \
  Days of Construction: #(datetime.today() - start-date).days() \ \
  Source: #link(source_url)[#source_label] \

  #pagebreak()
  #set page(fill: page-background, footer: outline-page-footer())
  #outline()
]

#let body(content, source_url: "https://github.com/lince-social/lince") = [
  #set text(
    lang: doc-lang,
    region: doc-region,
    weight: "regular",
    size: 12pt,
    fill: text-color,
  )
  #set page(
    fill: page-background,
    margin: (top: 1in, bottom: 1in, left: 1in, right: 1in),
    footer: body-page-footer(),
  )
  #set heading(numbering: "1.")
  #let current-major = context hydra(1, skip-starting: false)
  #set page(
    fill: page-background,
    header: body-page-header(current-major),
    footer: body-page-footer(),
  )
  #show heading: block.with(
    spacing: 2em,
  )

  #content
]

#let book(
  content,
  title: "Instinto",
  subtitle: "",
  start-date: datetime(year: 2025, month: 12, day: 15),
  author: "Eduardo de Melo Xavier",
  source_url: "https://github.com/lince-social/lince",
  source_label: "Lince",
  logo_dark: "../media/logo/white.svg",
  logo_light: "../media/logo/black.svg",
) = [
  #frontmatter(
    title: title,
    subtitle: subtitle,
    start-date: start-date,
    author: author,
    source_url: source_url,
    source_label: source_label,
    logo_dark: logo_dark,
    logo_light: logo_light,
  )
  #body(content, source_url: source_url)
]
