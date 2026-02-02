#import "../../../components/major.typ": major
#import "../../../components/task.typ": task
#import "@preview/cheq:0.3.0": checklist
#show: checklist

#major(
  "Tasks",
  $"// TODO"$,
  message: "Don't hallucinate while you implement the following features...",
  by: "Not me",
)

#task(
  "Canvas",
  contributors: ("N1",),
)[
  Make an expansive 2d canvas, views dragging to adjust the position.
  When im in any collection, i can drag and drop my views, resize them, and their positions and size it all saved.
  They can overlap and whichever View was meddled with last will have the highest Z index.
  There must be a button to order them to not be stacked, and an undo button.
  The press of the order button makes them fit the screeen as much as possible wrapping downwards
]

#task(
  "I can have a view that is the Creation Modal of any table",
  contributors: ("N1",),
)[
  One of the Views of any collection is the creation modal, now a View. So the person will see tables and next to it the component used to create a new record, so they will be able to create new records easily.
  This is good for todo behavior, being able to pin the view of record creation means a quick todo creation
]

#task(
  "Nextcloud integration",
  contributors: ("N1",),
)[
  Integração com nextcloud ou algum provedor de cloud pra sincronizar DNAs entre dispositivos
]

#task(
  "Note-taking-like",
  contributors: ("N1",),
)[
  Ter uma forma boa de editar notas, conectando possivelmente notas de objetivos com karma, pra que cada workflow tenha sua justificativa e possa-se criar primeiro os objetivos e completar eles com os passos pra chegar lá
]

#task(
  "Karma Refactor",
  contributors: ("N1",),
)[
  Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6.
  Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2
]

#task(
  "Extensions",
  contributors: ("N1",),
)[
  Be able to receive information about ESP and put it into a Karma condition.
]

#task(
  "Collection's Views CRUD",
  contributors: ("N1",),
)[
  Be able to:
  - Update view's name, query
  - Add views
  - Remove Views
  - Create Views
]

#task(
  "Be able to Pin Views",
  contributors: ("N1",),
)[
  I can pin a View of Collection A when I am in collection B, making it appear on the screen with higher Z index and stuck to a place
]

#task(
  "Execute single Karma with visualization on the condition being evaluated",
  contributors: ("N1",),
)[
  Maybe a modal
]

#task(
  "Cub",
  contributors: ("N1",),
)[
  Just like rustlings, a tutorial that you code Rust to fix bugs and learn we can have Cub. It will teach you things and ask you to fix stuff maybe to advance forward in using Lince and creating a useful DNA.
]

#task(
  "syntax highlighting and lsp (TreeSitter?) for Commands",
  contributors: ("N1",),
)[
  Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash, if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.
]

#task(
  "JSON Endpoints",
  contributors: ("N1",),
)[
  - backend retorna dados em json
  - backend aceita dados em json pra criar nas tabelas
]

#task(
  "IA de recomendação de Karma",
  contributors: ("N1",),
)[
  fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...
]

#task(
  "Shift de Ids",
  contributors: ("N1",),
)[
  Caso ids em cada dna ainda se mantenham em um INT autoincremental: Ajustar Ids quando deleto algo deixo espaços vazios, fazer um shift pra eles se apertarem em direção ao zero, pra ficar mais fácil de digitar numeros menores. Mudar todos os ids que referenciam os que mudaram, em karma expressions
]

#task(
  "Table component",
  contributors: ("@chicogborba",),
)[
  Table needs to have these properties:
  - Column Resize (saving information on individual columns' size)
  - Word wrapping or not (option to word wrap in configuration)
  - Editable: edit in place, in the cell, not changing the ui much, just turning the cell into an editable field.
  While also being able to edit a cell occupying a resizeable portion of the screen, so you can maximize/fullscreen a cell to edit it like a text editor.
  - Delete row (with confirmation, by default, with option in configuration to not ask)
  - Sorteable (On runtime, updates should respect this sort, even though data comes with another sort or unsorted
]

#task(
  "New Logo Items: First Batch",
  contributors: ("Nika", "@xaviduds"),
)[
  - [/] New vetorized logo
  - [ ] Hering's Super Cotton for t-shirts
  - [ ] Stickers: get tip from Nika
  - [ ] 3D Keychain Items: \@tecnopuc_crialab
]

#task(
  "Lince Institute",
  contributors: ("N1",),
)[
  - [ ] Consolidate the 'Ata de Fundação' and 'Estatuto Social'
  - [ ] Use the 'Ata de Fundação' and 'Estatuto Social' to create the 'Associação privada' called 'Instituto Lince'
  - [ ] CNPJ (Cora (more famous) or Conta Simples)
  - [ ] Bank Account (which bank? Banco do Brasil?)

  In Brazil there is a model of legal organization called Associação privada with a non-profit declaration.
  It has the best legal model for Lince in the several years to come. It's main purpose is to be a legal body
  in case the project needs to be represented or have a relationship with other parties.
  These relationships include donations, contracts with developers, legal operational reports and more.

  The legal category of the organization is an 'Associação privada'. The plan is for the public legal names 'Razão Social' and 'Nome Fantasia'
  to be 'Insituto Lince'. And to be commonly known when reffering to the project as Lince and have the institute as a supporting piece.

  The following documents explain all the papers needed for the entire lifecycle of the Instituto Lince, the legal
  entity in Brazil to operate and represent the bureaucracy of the project.

  *Ata de Fundação*
  Document used to officially create the organization, needs at minimum the founder Eduardo another associate (they dont need to be
  involved in the project after the creation).

  *Estatuto Social*
  The rules and permissions the project has. The rights and duties it proposes to follow.

  *Procuração*
  Document to be updated and adapted whenever a new legal/financial task needs to be complete, and it's not done by the Executive Director.
]

#task("Find Social Media Manager", contributors: ("N1",))[
  Train them into using Typst to automate it, make simple content that delivers only
  important news and concepts with mininum words, connecting it with trying out Lince.

  Youtube (this_month_in_lince)

  Insta (social_media_posts)

  TikTok

  Website?? (Blogpost)
]

#task("Migration with Fallback", contributors: ("@xaviduds (doing)",))[
  There should be a way with an update to ask the user if they want to apply a migration, the system needs to work with a certain db schema and those
  breaking changes should be able to be applied with:
  - A migration by the user's choice
  - Automatic backup of the db
  - Automatic reversal if it doesnt work

  SQLx has a migration feature, what might happen is that the database layer needs to change to make the tables reflect an Infrastructure Struct
  that reflects the Domain. Currently the schema of the db is a bunch of strings...
]

#task("Hello", contributors: ("@DiogoTeixeiraDEV",))[
  Frequency has two features to be done so we can complete it:

  *Catch Up Sum:*
  When a frequency hasnt been activated for a long time, like for a 1 Day frequency with a next_date stuck
  three months ago, if something references it, every Karma Delivery (60s) will update it to one day closer to tomorrow (2 months 29 days now).
  The catch_up_sum is something that takes all of the possible times the frequency would activate and moves the next_date until it reaches stability.
  catch_up_sum == 0 => dont do anything, just calculate frequency normally one time.
  catch_up_sum => positive, make the next_date jump the number of times the value of catch_up_sum, never jumping if next_date is already in the future.
  In other words: if its 1, its the same as zero, you jump the next_date one time based on the frequency (1 day) and go on.
  If it's two, you jump two times so it would go from 3 months ago to 2 months and 28 days.
  If its negative dont do anything.

  *Days of the Week:*
  There already is a commented try at this in the frequency function. The goal is to make something easy to write to say that it should fall in a day
  of the week. So if the frequency only contains info about jumping every monday and tuesday then the day_of_week would be something like `1, 2` or `12`
  or something else, you who knows.
  If the frequency is `months: 2, day_of_week: 5` it will first jump to the next friday, then jump two months. Or maybe it should first go to the two
  months and then fall on a friday. There should be a mechanism to easily set a prefference between the two behaviors.

  Feel free to refactor this a lot. With those two unfinished parts the frequency will be able to cover many cases, if you have more periodicies in
  mind to cover even more cases please refactor.
]

#task("Deterministic Simulation Testing", contributors: (
  "@xaviduds",
  "@DiogoTeixeiraDEV",
))[

  DST is amazing!  The idea (I think) is to have three things:
  1. The Seed: the user's DNA (la ele).
  2. The Rules: What events should be bookmarked or stop the simulation?
  3. The Engine: How will this simulation happen? With the normal flow of time, or a tampered one? Connecting to the outside world with Commands?

  This way we can create futures shown to the user so they can see to the end of their Karma and catch bugs or unintended behavior.
  This is useful in finantial simulation, or for understanding the costs of time for doing tasks (like the Calendar feature).

  With DST we may duplicate the DNA to change it freely without affecting the user's data, or perhaps not changing persistent data at all,
  just manipulating data inside the program.

  TigerBeetle is the GOATED db for this, perhaps Lince can learn from it, fork it, or use it with a different schema for Transaction of Records.
  https://www.youtube.com/watch?v=JoYjji1DZCE
]
