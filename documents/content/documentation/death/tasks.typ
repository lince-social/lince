#import "../../../components/major.typ": major
#import "../../../components/task.typ": task, task-board
#import "@preview/cheq:0.3.0": checklist
#show: checklist

#major(
  "Tasks",
  $"// TODO"$,
  message: "Don't hallucinate while you implement the following features...",
  by: "Not me",
)

#let tasks = (
  task(
    "Editable cell",
    contributors: (("@xaviduds", "wip"),),
    type: "Frontend",
  )[
    Each cell when hovered should:
    - [ ] Have an icon to edit it in a resizeable text buffer. Becoming a big modal that covers almost the entire screen when it opens.
    - [/] Be editable in place if clicked, maintainting the same size, but with a big text field, instead of just a text cell.
  ],
  task(
    "Table component",
    contributors: (("@chicogborba", "wip"),),
    type: "Frontend",
  )[
    I think this is the best way, sending the column size information to the db... but it's kinda weird?
    Table needs to have these properties:
    - [ ] Custom column width. If a table has too many columns we need to decide if we:
    (A) reduce that columns' size to fit the screen (hard)
    (B) make the central area with the tables be like a 2d canvas with vertical/horizontal scrollable space (maybe even zoom).
    If that's the case a (return to content is cool but for now ok not having).
    - [/] Column Resize: saves information on individual columns' size.
    For each collection, every view that is a table should be able to have each column with a custom width.
    - [ ] Have word-wrapping by default, in the cells, being able to toggle it in Configuration.
    - [ ] Any changes made to the db have all fit inside one or more migrations.
  ],
  task(
    "Lince Institute",
    contributors: (("@xaviduds", "todo"),),
    type: "Bureaucracy",
  )[
    - [/] Hire Lawyer: AFOSC.
    - [ ] Consolidate the 'Ata de Fundação' and 'Estatuto Social'
    - [ ] Use the 'Ata de Fundação' and 'Estatuto Social' to create the OSC Institute called called 'Instituto Lince'
    - [ ] CNPJ (Cora (more famous) or Conta Simples)
    - [ ] Bank Account (which bank? Banco do Brasil?)


    It's main purpose is to be a legal body
    in case the project needs to be represented or have a relationship with other parties.
    These relationships include donations, contracts with developers, legal operational reports and more.

    The Institute being OSC, 'Organização da Sociedade Civil' allows for donations to become tax reductions by the
    donating parties. Also opens the possibility of earning technology-based government programs' funding.

    The following documents explain all the papers needed for the entire lifecycle of the Instituto Lince, the legal
    entity in Brazil to operate and represent the bureaucracy of the project.

    *Ata de Fundação*
    Document used to officially create the organization, needs at minimum two people.

    *Estatuto Social*
    The rules and permissions the project has. The rights and duties it proposes to follow.

    *Procuração*
    Document to be updated and adapted whenever a new legal/financial task needs to be complete,
    and it's not done by someone of the Institute with permission.
  ],
  task(
    "New Logo Items: First Batch",
    contributors: (("Nika", "wip"), ("@xaviduds", "todo")),
    type: "Design",
  )[
    - [x] New vetorized logo | nika
      - [/] Stickers: get tip from Nika | duds
      - [x] Hering's Super Cotton for t-shirts | duds
        - [/] Embroider them with the logo: iguat | duds
    - [ ] Blender 3D logo | nika
      - [ ] 3D Keychain Items \@tecnopuc_crialab | duds
  ],
  task(
    "Karma Refactor",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6.
    Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2
  ],
  task(
    "Extensions",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    Be able to receive information about ESP and put it into a Karma condition.
  ],
  task(
    "Note-taking-like",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    Ter uma forma boa de editar notas, conectando possivelmente notas de objetivos com karma, pra que cada workflow tenha sua
    justificativa e possa-se criar primeiro os objetivos e completar eles com os passos pra chegar lá
  ],
  task(
    "IA de recomendação de Karma",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...
  ],
  task(
    "Execute single Karma with visualization on the condition being evaluated",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    Maybe a modal
  ],
  task(
    "Table Extra Ergonomics",
    contributors: (("N1", "todo"),),
    type: "Frontend",
  )[
    - Delete row (with confirmation, by default, with option in configuration to not ask)
    - Sorteable (On runtime, updates should respect this sort, even though data comes with another sort or unsorted
  ],
  task(
    "Frequency",
    contributors: (("@DiogoTeixeiraDEV", "wip"),),
    type: "Karma",
  )[
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
  ],
  task(
    "syntax highlighting and lsp (TreeSitter?) for Commands",
    contributors: (("N1", "todo"),),
    type: "Karma",
  )[
    Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash,
    if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.
  ],
  task(
    "Deterministic Simulation Testing",
    contributors: (("@xaviduds", "todo"), ("@DiogoTeixeiraDEV", "todo")),
    type: "Karma",
  )[
    DST is amazing!  The idea (I think) is to have three things:
    1. The Seed: the user's DNA (la ele).
    2. The Rules: What events should be bookmarked or stop the simulation?
    3. The Engine: How will this simulation happen? With the normal flow of time, or a tampered one? Connecting to the outside world with Commands?

    This way we can create futures shown to the user so they can see to the end of their Karma and catch bugs or unintended behavior.
    This is useful in finantial simulation, or for understanding the costs of time for doing tasks (like the Calendar feature).

    With DST we may duplicate the DNA to change it freely without affecting the user's data, or perhaps not changing persistent data at all,
    just manipulating data inside the program.

    TigerBeetle is the GOATED db for this, perhaps Lince can learn from it, fork it, or use it with a different schema for Transaction of Records.
    https://youtu.be/sC1B3d9C_sI?si=_HbNMQ9NVegLyS2a
    https://www.youtube.com/watch?v=JoYjji1DZCE
  ],
  task(
    "Canvas",
    contributors: (("N1", "todo"),),
    type: "Frontend",
  )[
    Make an expansive 2d canvas, views dragging to adjust the position.
    When im in any collection, i can drag and drop my views, resize them, and their positions and
  ],
  task(
    "I can have a view that is the Creation Modal of any table",
    contributors: (("N1", "todo"),),
    type: "Frontend",
  )[
    One of the Views of any collection is the creation modal, now a View. So the person will see tables and next to it the component used to create a new record, so they will be able to create new records easily.
    This is good for todo behavior, being able to pin the view of record creation means a quick todo creation
  ],
  task(
    "Collection's Views CRUD",
    contributors: (("N1", "todo"),),
    type: "Frontend",
  )[
    Be able to:
    - Update view's name, query
    - Add views
    - Remove Views
    - Create Views
  ],
  task(
    "Be able to Pin Views",
    contributors: (("N1", "todo"),),
    type: "Frontend",
  )[
    I can pin a View. Saving that information in the Configuration (maybe an intermediate table).

    Pinned Views appear on the screen independently of the active Collection,
    making it appear on the screen with higher Z index and stuck to a place.

    - [x] Be able to Pin/Unpin Views.
    - [ ] Have the pinned view be able to be resized and moved. Persist that information in the Pin Collection table.
    - [ ] Default to putting the pinned view on the bottom right corner.
    - [ ] Make sure the Pin border and the unpin button doesn't take too much space, as little as possible.
    Maybe a thin line border and an unpin button on hover. Now its like a thick window decoration.
  ],
  task("Command buffer", contributors: (("N1", "todo"),))[
    When a Command is run, an 'sh' command is spawn. One can see the stdin/out/err if looking at the binary's console.

    That limits the interaction with the command; the read of the output/err and the usage of stdin, interactive programs...

    The task involves a bidirectional connection, probably with tokio's rx/tx.

    The streaming is of the stdin/out/err between the function that executes the 'sh' command and the frontend being accessed.

    *Tasks*
    - [ ] Bidirectional channel between Command runner and a Command watcher. The Command runner sends information about the Command,
    the Command watcher sends information about the user's interaction.
    - [ ] GPUI component that acts as a watcher to receive and send Command information.
    This component must be able to be set as a View in any Collection.

    This looks like a streaming of text in a box, like an agent chat.

    *Bonus Points*

    - [ ] Goated is the one that can maintain the shell's text highlighting.
  ],
  task("Lynx alive in Website", contributors: (("N1", "todo"),))[
    Make a _Lynx canadensis_ 3D model inside the website. While the website is boring, the Lynx is very alive.
    By default it doesnt mess with anything. But if you pet it a lot it becomes hiperstimulated and with that energy it becomes hyperactive.
    It plays with the components of the screen, scrambles them, munches them, removing a part of the top bar. It follows you around the screen.
    If you try to click some links it moves them away, like a Turk icecream man that never lets you get the icecream.

    The model of the Lynx probably needs to be done in Blender, then put in Three.js or Bevy WebAssembly WebGPU.

    If you refresh the screen it goes back to normal.
  ],
  task("Online Shop", contributors: (("N1", "todo"),))[
    - [ ] Make an online shop managed by Lince.
    When purchases arrive, all the Needs to complete the service to the customers are created, including:
    - [ ] Ordering the white label items in the case of clothing, embroidering them (maybe both by same vendor);
    or printing stickers/keychains.
    - [ ] Creating the transport Need.
    - [ ] NF-e.

    For items put:
    - [ ] T-shirts
    - [ ] Stickers
    - [ ] 3D Keychain Accessory
    - [ ] Hoodies

    Depending on the cost it might be good to make either on demand by third party
    (JIT when an order arrives one item is bought) or
    produce in batches.
    - [ ] Give the option for the person to wait until a new batch is created for a lower cost or for them to
    buy 1 or more from "scratch". For the latter we will charge more because making one single item is more expensive.
  ],
)

#task-board(tasks)
