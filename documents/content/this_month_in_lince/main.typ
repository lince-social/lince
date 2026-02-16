#import "../documentation/sickness/sickness.typ": roadmap
#import "@preview/cheq:0.3.0": checklist
#show: checklist

#import "@preview/touying:0.6.1": *
#import themes.simple: *
#show: simple-theme.with(aspect-ratio: "16-9")

#let folder_name = sys.inputs.at("dir", default: "2025_12")

#let year = folder_name.split("_").at(0)
#let latest = json(year + "/" + folder_name + ".json")

#let date_parts = folder_name.split("_")
#let current_year = date_parts.at(0)
#let current_month = date_parts.at(1)
// ----------------------------

#let dark = true
#let bg = if dark { rgb(20, 20, 20) } else { white }
#let fg = if dark { white } else { black }

#set page(fill: bg)
#set text(fill: fg)

#set text(
  lang: "en",
  font: "New Computer Modern Math",
  size: 20pt,
)

#set heading(numbering: none)

#let languages = (
  "pt-BR": "PT",
  "zh-CN": "中文",
  "en": "EN",
)

#let strings = (
  "title": (
    "pt-BR": "Este Mês na Lince",
    "zh-CN": "本月在林斯",
    "en": "This Month in Lince",
  ),
  "growth": (
    "pt-BR": "Crescimento",
    "zh-CN": "成长工作",
    "en": "Growth",
  ),
  "development": (
    "pt-BR": "Programação",
    "zh-CN": "开发",
    "en": "Programming",
  ),
  "roadmap": (
    "pt-BR": "Cronograma",
    "zh-CN": "路线图",
    "en": "Roadmap",
  ),
  "see-you": (
    "pt-BR": "Até o próximo mês!",
    "zh-CN": "下个月见！",
    "en": "See you next month!",
  ),
)

#let render-i18n(item) = {
  for (code, label) in languages {
    item.i18n.at(code).title

    if item.i18n.at(code).description != "" {
      ": " + item.i18n.at(code).description
    }

    linebreak()
    linebreak()
  }
}

#let resolve-image-path(image-name) = {
  if image-name.starts-with("media/") {
    "/documents/" + image-name
  } else if image-name.starts-with("year/") {
    year + "/" + image-name.slice(5)
  } else if image-name.contains("/") {
    "/documents/media/" + image-name
  } else {
    year + "/" + image-name
  }
}

#let act(str) = [
  ==
  #align(center + horizon)[
    #text(size: 32pt, weight: "bold")[
      #for (code, _) in languages {
        strings.at(str).at(code)
        if code != languages.keys().last() { " | " }
      }
    ]
  ]
]

#let parts(str) = [
  #for i in latest.at(str) [
    == #for (code, _) in languages {
      strings.at(str).at(code)
      if code != languages.keys().last() { " | " }
    }

    #align(center + horizon)[
      #if "image" not in i or i.image == "" [
        #render-i18n(i)
      ] else [
        #grid(
          columns: (1fr, 1fr),
          gutter: 40pt,
          align(center + horizon)[
            #render-i18n(i)
          ],
          align(center + horizon)[
            #image(resolve-image-path(i.image), width: 90%)
          ],
        )
      ]
    ]
  ]
]

// --- TITLE SLIDE ---
= #for (code, _) in languages {
  strings.at("title").at(code)
  if code != languages.keys().last() { " | " }
} \ #current_year - #current_month

#act("growth")
#parts("growth")

#act("development")
#parts("development")

#act("roadmap")
#text(size: 20pt)[
  #roadmap
]

=
#slide[
  #align(center + horizon)[
    #text(size: 32pt, weight: "bold")[
      #for (code, _) in languages {
        strings.at("see-you").at(code)
        linebreak()
      }
    ]
  ]
]
