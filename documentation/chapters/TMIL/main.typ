#import "../sickness.typ": roadmap

#import "@preview/cheq:0.3.0": checklist
#show: checklist

#import "@preview/touying:0.6.1": *
#import themes.simple: *
#show: simple-theme.with(aspect-ratio: "16-9")

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

#let data = json("202512/data.json")
#let latest = data.sorted(key: d => int(d.year) * 12 + int(d.month)).last()

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
    "en": "Roadmap",
    "pt-BR": "Roadmap",
    "zh-CN": "路线图",
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
            #image(i.image, width: 90%)
          ],
        )
      ]
    ]
  ]
]

= #for (code, _) in languages {
  strings.at("title").at(code)
  if code != languages.keys().last() { " | " }
} \ #latest.year - #latest.month

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
