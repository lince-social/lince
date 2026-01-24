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
  title: "I can have a view that is the Creation Modal of any table",
  contributors: ("xaviduds",),
)[
  One of the Views of any collection is the creation modal, now a View. So the person will see tables and next to it the component used to create a new record, so they will be able to create new records easily.
  This is good for todo behavior, being able to pin the view of record creation means a quick todo creation
]

#task(
  title: "Collection CRUD",
  contributors: ("xaviduds",),
)[
  Enable CRUD operations for collections.
]

#task(
  title: "Nextcloud integration",
  contributors: ("xaviduds",),
)[
  Integração com nextcloud ou algum provedor de cloud pra sincronizar DNAs entre dispositivos
]

#task(
  title: "Note-taking-like",
  contributors: ("xaviduds",),
)[
  Ter uma forma boa de editar notas, conectando possivelmente notas de objetivos com karma, pra que cada workflow tenha sua justificativa e possa-se criar primeiro os objetivos e completar eles com os passos pra chegar lá
]

#task(
  title: "Karma Refactor",
  contributors: ("xaviduds",),
)[
  Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6. 
  Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2
]

#task(
  title: "Extensions",
  contributors: ("xaviduds",),
)[
  Be able to receive information about ESP and put it into a Karma condition.

  Dont remember what esp is...
]

#task(
  title: "Collection's Views CRUD",
  contributors: ("xaviduds",),
)[
  Be able to:
  - Update view's name, query
  - Add views
  - Remove Views
  - Create Views
]

#task(
  title: "Be able to Pin Views",
  contributors: ("xaviduds",),
)[
  I can pin a View of Collection A when I am in collection B, making it appear on the screen with higher Z index and stuck to a place
]

#task(
  title: "Execute single Karma with visualization on the condition being evaluated",
  contributors: ("xaviduds",),
)[
  Maybe a modal
]

#task(
  title: "Cub",
  contributors: ("xaviduds",),
)[
  Just like rustlings, a tutorial that you code Rust to fix bugs and learn we can have Cub. It will teach you things and ask you to fix stuff maybe to advance forward in using Lince and creating a useful DNA.
]

#task(
  title: "syntax highlighting and lsp (TreeSitter?) for Commands",
  contributors: ("xaviduds",),
)[
  Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash, if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.
]

#task(
  title: "JSON Endpoints",
  contributors: ("xaviduds",),
)[
  - backend retorna dados em json
  - backend aceita dados em json pra criar nas tabelas
]

#task(
  title: "IA de recomendação de Karma",
  contributors: ("xaviduds",),
)[
  fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...
]

#task(
  title: "Shift de Ids",
  contributors: ("xaviduds",),
)[
  Caso ids em cada dna ainda se mantenham em um INT autoincremental: Ajustar Ids quando deleto algo deixo espaços vazios, fazer um shift pra eles se apertarem em direção ao zero, pra ficar mais fácil de digitar numeros menores. Mudar todos os ids que referenciam os que mudaram, em karma expressions
]

#task(
  title: "Table component",
  contributors: ("xaviduds",),
)[
  Table needs to have these properties:
  - Column Resize (Saving information on individual columns)
  - Word wrapping or not ( info in configuration)
  - Editable: edit in place, in the cell, not changing the ui much, and also being able to edit a cell occupying a resizeable portion of the screen, so you can maximize/fullscreen a cell to edit
  - Delete row (with confirmation, by default, with option in configuration to not ask) 
  - Sorteable (On runtime, updates should respect this sort, even though data comes with another sort or unsorted
]
