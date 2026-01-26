#import "@preview/touying:0.6.1": *
#import themes.simple: *

#show: simple-theme.with(aspect-ratio: "4-3")

#set text(
  font: "New Computer Modern Math",
  size: 22pt,
)

#let json_name = sys.inputs.at(
  "json",
  default: "0001_lince_overview/0001_lince_overview.json",
)
#let data = json(json_name)


#let corner-logo = image("../../../assets/preto_no_branco.png", width: 18pt)

#show page: it => (
  it
    + place(
      top + right,
      inset: 10pt,
      corner-logo,
    )
)


#let languages = (
  "pt-BR": "PT",
  "zh-CN": "中文",
  "en": "EN",
)

#let render-slide(slide-data) = {
  align(center + horizon)[
    #stack(
      spacing: 20pt,
      ..languages
        .keys()
        .map(lang => {
          slide-data.i18n.at(lang)
        }),
    )
  ]
}

#for post in data.posts [
  #slide[
    #align(center + horizon)[
      #rect(stroke: 2pt, inset: 20pt)[
        #set text(weight: "bold", size: 30pt)
        #post.title.at("pt-BR") \
        #post.title.at("zh-CN") \
        #post.title.at("en")
      ]
      #v(20pt)
      #text(
        size: 15pt,
        fill: gray,
      )[Instagram \@lincesocial | GitHub \@lince-social/lince]
    ]
  ]

  #for slide-content in post.slides [
    #slide[
      #render-slide(slide-content)
    ]
  ]
]
