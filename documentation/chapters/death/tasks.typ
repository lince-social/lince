#import "../../components/chapter.typ": major
#import "../../components/task.typ": milestone, task, task-board
#import "@preview/cheq:0.3.0": checklist
#show: checklist

#major(
  "Tasks",
  $"// TODO"$,
  message: "Don't hallucinate while you implement the following features...",
  by: "Not me",
)

#let milestones = (
  milestone("0.0.0 Non crazy-frog people", (
    task(
      "0.0 Usable by non-crazy-frog people, but still technical",
      contributors: (("@xaviduds", "wip"),),
    )[
      - [ ] Creation Component - Autocomplete in each text field. Have a way for me to configure in Rust, in a hard-coded way, when typing in fields in a creation component what fields should trigger autocomplete for what tables. Example: in the creation component for the View table i should be able to type in Name or Query and search only rows of Views where any column has the same full text search part as the one im writting in that field. If click on a line of autocomplete, or if I type enter (with a current line selected by the autocomplete), all the empty fields in my Creation component should have the values of the line selected. If I then edit another field that should also trigger autocomplete, and if i hit enter in that it will also replace the value of the current input field with the searched one and the empty ones. That is the default behavior for creation components.
        - [ ] Some Tables though have another hardcoded configuration:
          - [x] for Karma i have the consequence_id and condition_id fields in creation component that should search not in karma, but in karma_consequence or karma_condition respectivelly.
            - [ ] There is some
          - [x] In the Collection Bar, for every collection row that is hovered, a plus sign button should be shown. If clicked it will open a creation modal of the CollectionView intermediate table. This modal should have autocomplete for Collection columns in the collection_id field and the same for view_id.
            - [/] Make the search not block the ui, its kinda janky, the continuous search shouldn wait for the query to finish. Or maybe dont do that, i think it might be the rendering the slow part. Make it more efficient first.
            - [/] I clicked the + button and typed the name of a View, it filled the view id correctly in the CollectionView creation component. But when i hit enter, hitting enter again to create the View i think behaved as if i hit enter again, when i select something, make the autocomplete part of the component gone, so that if i hit enter once to select it, hiting enter again is expected to create the CollectionView.
            - [/] Also, after I created it, the collections up top didnt change, only the views in main.
      - [x] One can, like an LSP completion, go through the completions, with arrow keys or if vim mode: arrow keys + ctrl+p for previous (if in the first one go to the last, vice versa) and ctrl+n for next. If the user just started typing, the first one is the selected one.
      Record
      So the body of Record might be repertitive, like: Habit, Work.
      One might type only H and have [Habit, Work], [Habit, Health]... appear.

      - [ ] Make a View with the 'cheat' Query that is a component with all the Operations and Keybinds
      - [x] Keybinding consultation component with ease of access (maybe when operation input is focused in insert mode)
      - [ ] Editable Cell Ergonomics:
        - [ ] In normal and vim mode, being able to select text to:
          - [ ] Delete
          - [ ] Copy to clipboard
          - [ ] Paste from clipboard
      - [ ] Moving around pinned views is very weird, it's slow, and sometimes it feels like i can only move it so much in one direction, resizing is weird too. This happens with the Editable Cell modal so its probably the skeleton Area that has resize and moveable that is weird. Make sure you are optmizing it 100% and only saving the position and resize when the user stops doing that action, releasing the mouse.
    ],
  )),
  milestone(
    "1.0.0: Dogfooding",
    (
      task(
        "Next Planning Talking Points",
        contributors: (("N1", "wip"),),
      )[
        - [ ]
      ],
      task("Tasks no Servidor")[
        - [x] Rodar a lince no servidor
        - [ ] Integrar localmente
        - [ ] Apontar pra outro lugar que não seja localmente
          - [ ] `
        

      ],
      task(
        "New Logo Items: First Batch",
        contributors: (("Nica", "wip"), ("@xaviduds", "wip")),
      )[
        - [/] Stickers -> Rei do Sticker: 500 (250 b&w, 250 w&b) laminated. It will be 5x5cm. | Duds
        - [/] Create the digital Tshirt design. Decide if its only logo and text close together, or if its logo in the heart part of the chest and 'Lince' in the other, or maybe in the back. Maybe have the name of the person in the other part of the chest and Lince in the back. | Nica & Duds (Website)
          - [/] Embroider them with the logo: iguat | Duds
        - [ ] Blender 3D logo | Nica
          - [ ] 3D Keychain Items \@tecnopuc_crialab | Duds
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
      task("Tutorial: First Steps", contributors: (("@xaviduds", "wip"),))[
        This is a document that is supposed to teach someone what Lince is.

        The current documentation is a mix of teaching newcomers and documenting in high detail, we want to separate the two into different contributions so they fit each case better.

        This should have four ways to consume easiest first:
        - [/] Video (Not too long)
        - [/] Text (Blogpost)
        - [ ] HTML version
        - [ ] TUI version (will be the sole hands-on tutorial for some time)
      ],
      task("Extensions", contributors: (
        ("@chicogborba", "wip"),
        ("@xaviduds", "todo"),
      ))[
        Be able to use a View someone else made.
        How do we use it? Editing binary is not tha wae.
        Can i write it in a language that is not Rust? An interface library that is not the one used by the GUI? Can I run ratatui?
      ],
      task("SQL Query Editor GUI", contributors: (("@chicogborba", "todo"),))[
        This could be replaced with the connector that is used for getting data into the sandbox the extension uses. If i need to select what data i want it is pretty similar to the workflow of making an sql query in a GUI-like way.

        - [ ] Being able to drag and drop columns to change the order of the SQL column being fetched.
        - [ ] In the GUI SQL Editor (excel-filters-like) have an order of priority for the rules of each column:
          - [ ] Order by (Ascending or Descending).
          - [ ] Filter by excluding or including. Example: only lines with certain characters at the start/end/anywhere of the line in that column: SELECT \* FROM record WHERE LOWER(body) LIKE '%task%' is the pseudo sql one might write to only get lines that contain any casing of 'task' in it.

          Maybe we can combine several including and several excluding to make it very versatile and allow for many different workflows.

          Wireframe suggestion of UI we first thought, probably very bad and stinky and needs complete rethinking:

          Having one modal open somewhere on the screen when filtering the table, if every column has a filter icon it might get repetitive since we always open the same modal when clicking it, we can put somewhere fixed on tables, or not, to hide it in the columns with hover.

          After clicking it we go to the modal that shows the columns, probably with empty rules at first. We might see a button with a plus sign, clicking it will show us the options of rules we can add to this column. If its a column wih limited options like an order by that can only be Ascending or Descending it can be a dropdown. If its something like an exclude/include it might need an input field.

          It would be awesome, maybe not in 1.0, to have autocomplete in this field. That would probably involve making some sort of real time query in db to get the data that is similar to what the user is typing. But it still needs more tought.

          I think it might be necessary to recycle some of these components to help people to create SQL Views without having to know SQL. In the Create View modal, or in each Collection row when hovering over the Views we might see a '+' button for a special View creation modal that is more graphical, with checkboxes to select the columns wanted from what table. No need to allow for joins, just basic select from where, maybe some of the logic of the filter with sorting.
          uaoisiuashiuash

        - [ ] Maybe todo?
      ],
      task("Schema Evolution", contributors: (("Vini", "wip"),))[
        This is a thinking task.

        There is a fine balance in making something generic and not bloated that isn't slow and painfull to work with. If you try to cover too many cases by default, with too many properties in the baseline app many people will not use it.

        We want the users to be able to set custom properties. It's closely tied to database normalization. How can we best keep properties in the main tables that will certainly be used? And be able to extend the properties used to fit various needs.

        Example: Lince doesn't currently have a cost system. If I modeled something like a Record of 'Exercise', I cant say that it has a cost of 30minutes. How much should Lince have by default for Record's properties? Like a cost system, physical location, starting and end date...

        How much should be like a json (in a text property in db) that we let the user just invent some stuff and put it there? And how much should be a default behavior of the Lince app? Do we care about making some property that most people will not use but that doesnt hurt to have? We could make more versions that are simpler to not have bloat, but that adds work to maintain those versions, right?

        This is a thinking task.
      ],
      task(
        "Karma Refactor",
      )[
        Karma Conditions poderem ser referenciadas em outras conditions tipo kd2 + kd6.
        Garantir que seja possível ter cadeias infinitas de condições: karma: kd2 = kd6 = ks2
      ],
      task(
        "Endpoint",
      )[
        - Be able to receive info about actions like changing a record Quantity, activating commands, running Karma through endpoints. That way any device can be a client, even an ESP.
      ],
      task(
        "Note-taking-like",
      )[
        Ter uma forma boa de editar notas, conectando possivelmente notas de objetivos com karma, pra que cada workflow tenha sua justificativa e possa-se criar primeiro os objetivos e completar eles com os passos pra chegar lá
      ],
      task("Design: Profile like", contributors: (("Nica", "todo"),))[
        Currently, lince feels like only a personal offline todo app. Because that's what it currently is.

        In the future, we want people to use it as their business/company's profile, to buy/sell/donate stuff.

        That could be a Collection that you make public. People can access your collection and see all the data you have in the Views shown. One can have a View on top that contains a lot of information about you: Profile picture, background header, email, name, history of transactions to show credibility...
        Then, below that are several Views showing part of what you wish to sell/donate. It might have a View of things you Need and would accept a donation of it.

        Feels a little no-code-ish. And that might be a good thing for non-technical people. There are ways to compose your Views with raw SQL and GUI so lot's of people can use easily or with high control.
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
      task("Shortcut focus operation input")[
        Ctrl-K will focus on it, anywhere that im in, it focuses it.
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
      task("Organ Management")[
        Your data is called a DNA, your Lince instance is called a Cell. Many Cells form a group called an Organ. This Organ can be for your family, friends, company, party, whatever. People can use it to share part of their Needs public in this Organ, and Contribute to the Needs of the Organ. Lince is all of the Cells and Organs are working together for eachother's Needs.

        Your day-to-day using Lince is to mostly to manage your Cell. We can to create the feature to manage Organs.

        We must find a way to understand how to host each Organ. One should be able to make their computer receive updates from everybody of the Organ, be like a server. Or maybe connect only in p2p way. It would be cool to be able to connect to something that is always on, to elect a central point for the Organ. If I update my Needs, other people should see it in real time, not only if the host's personal Cell is up.

        - [ ] Create
          - [ ] I can create an Organ, inviting Cells to participate in it.
        - [ ] Read:
          - [ ] I can see all the Organs my Cell is part of.
          - [ ] I can see one Organ with all it's information.
        - [ ] Update
          - [ ] I can change the Organ's information; if I have Admin permission?
          - [ ] I can change in my own DNA what information is public with what Organ, anonymously or not. It's cool if I can set something public/private with Karma, this way someone that works transporting people can see that a Need has become public, and it is of transport from A to B. One other way is to always set it to public and make Karma change it's quantity, so those that are looking to Contribute with that transport will only look at things that have negative quantities: are Needs of people.
        - [ ] Delete
          - [ ] Delete an Organ, with some steps of bureaucracy like having consent among certain participants. One can always leave the Organ.
      ],
    ),
  ),
  milestone(
    "Minor Version: AI",
    (
      task("Init: Integration")[
        - [ ] Integrate with the majority of AI models, local or remote through API.
      ],
      task(
        "View Creation",
      )[
        In the HTML version, have a way to put in the Sanboxes a component that was made by an AI after a prompt.
      ],
      task(
        "Karma Recommendation, Agentic and Tinkerer",
      )[
        The AI model looks at the user's data that the user marks as 'Allowed For AI'. The Karma, Records, Transfers, etc are analysed to sugest, in the Ask mode, to automatically change, in the Agent mode, or to simply do the optimizations it think you'll need, in Tinkerer mode.

        - [ ] Agent driven change by the user's request. When they ask the AI to change some data it performs the task.
        Bringing a short feedback about the success/failure, allowing the user to try again or look at the data and change it by hand.

        The workflows of automatic recommendation (with full access for instant change or asking for permission) must be:
        - [ ] Karma: for habits or purchases.
        - [ ] Records.

        fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...
      ],
    ),
  ),
  milestone("Minor Version: Nerdmaxxing", (
    task(
      "Execute single Karma with visualization on the condition being evaluated",
    )[
      Maybe a modal
    ],
    task("Command buffer")[
      - [ ] The command buffer doesnt always appear. At least when running the command with Operation Input.
      - [ ] Maintain the shell's text highlighting. Maybe using tree-sitter?
    ],
    task(
      "Command in several languages",
    )[
      syntax highlighting and lsp (Tree-Sitter?) for Commands
      Being able to see based on the language syntax highlighting. So if in a Command block there is not a language set default to bash,
      if there is rust use the highlight for Rust, use lsp to see if its wrong, be able to run every command and see the result.
    ],
  )),
  milestone("Minor Version: Ultra Rendering Rock n' Roll", (
    task("Digital/Real-World Maps")[
      Being able to see the world or a digital space with it's actors and needs/contributions.
      - [ ] One can see the world as a plane with lines for the streets.
        - [ ] Bonus points for terrain data, elevation, like mountains. With that in rendering we can portrait a more accurate picture of the world and also use the elevation to show the Needs and Contributions in a 3d way. If there are a lot of Needs in one area that is like a mountain visually.
        - [ ] Integrate that with Transfer Proposal. Being able to accompany the whole process through the maps, like a delivery; understanding who is closest to Contribute to your Need.

        https://github.com/orgs/Far-Beyond-Pulsar/discussions/40

        Maybe the way to go is using a game engine in gpui like Pulsar if it allows for the rendering of a Component in a canvas or something similar to display like a game level.
    ],
    task("5D Views")[
      3D so we can view a world. 4D so we can watch it's history, 5D so we can compare different scenarios in the future with their simulations of different outcomes.

      Visually this is actually 3D, but we can always put different 3D things side by side to compare between stages of the same timeline (4D) or 5D for different timelines.

      If we change the Frequency one Karma uses we can see how it will affect our DNA and others too if we have Karma that involves their DNA.

      With such possibly 5D canvas we will be able to save things spatially. One line in a 2D table is a way of grouping several lines close to one another. If we have things spacially set out in 3D we can have one record be like an object visually, with several copies in sync with the db one. We might not need a table like a 2d plane on a 3D world, that might be too boring.
    ],
  )),
  milestone("Right Before Needed", (
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
  )),
)

#task-board(milestones)
