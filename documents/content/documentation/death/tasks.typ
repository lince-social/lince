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
    "Next Planning Talking Points",
  )[
    - [ ] Switching License from GPLv3 to MIT for ease of collaboration with other projects. Should we do it?
    Or changing it to WTFPL: https://en.wikipedia.org/wiki/WTFPL

    - [ ] Going through the excalidraw and critiquing with design. Make documentation of things there.
  ],
  task(
    "Lince Institute",
    contributors: (("@xaviduds", "todo"),),
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
    contributors: (("Nica", "wip"), ("@xaviduds", "wip")),
  )[
    - [x] New vetorized logo | Nica
    - [x] Decide on a font for the 'Lince' word and 'Instituto Lince'.: Instrument Serif
      - [/] How it is displayed in relation to the logo, if its logo on top and 'Lince' written on the bottom or on the right.
      - [/] Same thing for 'Instituto Lince'
      - [/] Stickers -> Rei do Sticker: 500 (250 b&w, 250 w&b) laminated or holographic. Make a design for it (may contain website lince.social).
      - [x] Buy Hering's Super Cotton for t-shirts | Duds
        - [/] Create the digital Tshirt design. Decide if its only logo and text close togeter, or if its logo in the heart part of the chest and 'Lince' in the other, or maybe in the back. Maybe have the name of the person in the other part of the chest and Lince in the back.
        - [/] Embroider them with the logo: iguat | Duds
    - [ ] Blender 3D logo | Nica
      - [ ] 3D Keychain Items \@tecnopuc_crialab | Duds
  ],
  task(
    "Lynx alive in Website",
  )[
    Make a _Lynx canadensis_ 3D model inside the website. While the website is boring, the Lynx is very alive.
    By default it doesnt mess with anything. But if you pet it a lot it becomes hiperstimulated and with that energy it becomes hyperactive.
    It plays with the components of the screen, scrambles them, munches them, removing a part of the top bar. It follows you around the screen.
    If you try to click some links it moves them away, like a Turk icecream man that never lets you get the icecream.

    The model of the Lynx probably needs to be done in Blender, then put in Three.js or Bevy WebAssembly WebGPU.

    If you refresh the screen it goes back to normal.
  ],
  task("Online Shop")[
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

  task(
    "Frequency",
    contributors: (("@DiogoTeixeiraDEV", "wip"),),
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
    "Table component",
    contributors: (("@xaviduds", "wip"),),
  )[
    I think this is the best way, sending the column size information to the db... but it's kinda weird?
    Table needs to have these properties:
    - [ ] Custom column width. If a table has too many columns we need to decide if we:
    (A) reduce that columns' size to fit the screen (hard)
    (B) make the central area with the tables be like a 2d canvas with vertical/horizontal scrollable space (maybe even zoom).
    If that's the case a (return to content is cool but for now ok not having).
    - [ ] Column Resize: saves information on individual columns' size.
    For each collection, every view that is a table should be able to have each column with a custom width.
    - [ ] Have word-wrapping by default, in the cells, being able to toggle it in Configuration.
    - [ ] Any changes made to the db have all fit inside one or more migrations.
  ],
  task(
    "Creation Component for any Table",
  )[
    - [x] I can have that creation component as a View. So the person will see tables and next to it the component used to create a new record, so they will be able to create new records easily. When i type 4c, or c4, or create 4 or record c or create record, ... in operation i will get in return the action the frontend must take, so the parse operation will be in charge of running commands, zeroing the quantities of records if an id is passed and matches any, this already exists. The feature we need to build well is to make this enum of crud operation or more, tables, and then with that we give to the gpui that enum, based on the return of the operation we activate on the screen a modal for creation of the specific table passed to the operation.
    - [x] Create the modal that is the Creation component of any Table. So for records it shows all fields except for Id and is like a forms. Find a way to get the columns of the table.
      - [x] It showed the modal, but the screen behind it was just background.
      - [x] Hitting tab didnt advance to the next input, ctrl-tab didnt go back. Arrow keys worked though..
      - [x] Hitting enter created it, but it closed the modal, please only close the modal when I the user presses esc so they can continue to create more. Make sure to update the current data with every creation.


    - [ ] Pin: This is good for todo behavior, being able to pin the view of record creation means a quick todo creation.
  ],
  task("SQL Query Editor GUI", contributors: (("@chicogborba", "wip"),))[],
  task(
    "Karma Refactor",
  )[
    Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6.
    Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2
  ],
  task(
    "Extensions",
  )[
    Be able to receive information about ESP and put it into a Karma condition.
  ],
  task(
    "Note-taking-like",
  )[
    Ter uma forma boa de editar notas, conectando possivelmente notas de objetivos com karma, pra que cada workflow tenha sua
    justificativa e possa-se criar primeiro os objetivos e completar eles com os passos pra chegar lá
  ],
  task(
    "Execute single Karma with visualization on the condition being evaluated",
  )[
    Maybe a modal
  ],
  task(
    "Table Extra Ergonomics",
  )[
    - Delete row (with confirmation, by default, with option in configuration to not ask).
    - [ ] Every row should have in the id column a button to delete the row with such id from the table in question. The
  ],
  task(
    "Canvas",
  )[
    Make an expansive 2d canvas, views dragging to adjust the position.
    When im in any collection, i can drag and drop my views, resize them, and their positions and
  ],
  task(
    "Collection's Views CRUD",
  )[
    Be able to:
    - Update view's name, query
    - Add views
    - Remove Views
    - Create Views
  ],
  task(
    "Be able to Pin Views",
  )[
    I can pin a View. Saving that information in the Configuration (maybe an intermediate table).

    Pinned Views appear on the screen independently of the active Collection, making it appear on the screen with higher Z index and stuck to a place.

    - [x] Be able to Pin/Unpin Views.
    - [ ] Currently the pin only appears when we change active Collection.
    - [ ] Have the pinned view be able to be resized and moved. Persist that information in the Pin Collection table.
    - [ ] Default to putting the pinned view on the bottom right corner.
    - [ ] Make sure the Pin border and the unpin button doesn't take too much space, as little as possible.
    Maybe a thin line border and an unpin button on hover. Now its like a thick window decoration.
  ],
  task("Command buffer")[
    - [ ] The command buffer doesnt always appear. At least when running the command with Operation Input.
    - [ ] Maintain the shell's text highlighting. Maybe using tree-sitter?
  ],
  task("Shortcut focus operation input")[
    Ctrl-K will focus on it with insert mode, when clicking it i think this is already the case.
  ],
  task(
    "Command in several languages",
  )[
    syntax highlighting and lsp (Tree-Sitter?) for Commands
    Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash,
    if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.
  ],
  task(
    "Deterministic Simulation Testing",
    contributors: (("@xaviduds", "todo"), ("@DiogoTeixeiraDEV", "todo")),
  )[
    DST is amazing! The idea (I think) is to have three things:
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
  task("Configuration")[
    Show configuration divided into sections, with toggle buttons, input fields so the user doesnt have to just edit a table.
    For colorschemes show the possible ones.
  ],
  task("Colorscheme")[
    Make figma design use variables, in the figma they will set a colorschme to watch things that will use those variable names with different values for the colors in each colorscheme. In compilation, have one struct with different values for each colorscheme, make figma export those variables to use in the application.
    Whenever a color is set, it will use the name of the variables from the design. We must in a very efficient way access those variables in the active colorscheme. That might be done every render, if we have access to that in global memory data it's better than accessing a json.

    That should also be done with scaling properties, so padding_s is maybe 2px or rem. The design will use the padding_s and the code will mimic it.
  ],
  task("Transfer Proposal")[
    One can create a Transference Proposal, saying they have a Contribution to make. That initial part, with their Record and a proposal of a Contribution can be enough data to be shown to the public. One doesn't need to set what type of Counter Contribution they receive for it. Whenever the one part of the transfer is written, it is ready to be shown.

    - [ ] I can set a proposal public with a certain frequency. One time or in a pendulum. I can set it public first to my main Organ.
  ],
  task(
    "IA de recomendação de Karma",
  )[
    fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...
  ],
  task("Digital/Real-World Maps")[
    Being able to see the world or a digital space with it's actors and needs/contributions.
    - [ ] One can see the world as a plane with lines for the streets.
      - [ ] Bonus points for terrain data, elevation, like mountains. With that in rendering we can portrait a more accurate picture of the world and also use the elevation to show the Needs and Contributions in a 3d way. If there are a lot of Needs in one area that is like a mountain visually.
      - [ ] Integrate that with Transfer Proposal. Being able to accompany the whole process through the maps, like a delivery; understanding who is closest to Contribute to your Need.

      https://github.com/orgs/Far-Beyond-Pulsar/discussions/40

      Maybe the way to go is using a game engine in gpui ike Pulsar if it allows for the rendering of a Component in a canvas or something similar to display like a game level.
  ],

  task("5D rendering")[
    3D so we can view a world. 4D so we can watch it's history, 5D so we can compare different scenarios in the future with their simulations of different outcomes.

    Visually this is actually 3D, but we can always put different 3D things side by side to compare between stages of the same timeline (4D) or 5D for different timelines.

    If we change the Frequency one Karma uses we can see how it will affect our DNA and others too if we have Karma that involves their DNA.

    With such possibly 5D canvas we will be able to save things spatially. One line in a 2D table is a way of grouping several lines close to one another. If we have things spacially set out in 3D we can have one record be like an object visually, with several copies in sync with the db one. We might not need a table like a 2d plane on a 3D world, that might be too boring.
  ],
)

#task-board(tasks)
