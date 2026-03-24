#import "../common/typst/document.typ": book
#import "components.typ": idea, major, slides-deck, slides-mode

#let content = [
  #major("What is Lince?", $???$)

  #idea(
    [Lince helps you meet any Need],
    visual-text: (
      "graph LR;
    A[What is a Need?];"
    ),
  )[
    How does it do that?

    We start by framing everything we do as a perception of attending to a Need.

    If it can be modeled in a Record, you can use it as a list of tasks, personal notes, items you need and more.

    If those tasks you Need to do or items you Need to get are cyclic in their nature you can see the Need of them with a certain Frequency. If that Frequency is a Condition for the Need to occur, let's say every day, you can setup a Consequence to see the that Need of eating an Apple every day. That's a Karma.

    With Karma you can create a two part automation. The Condition has to occur for a Consequence to follow. In the previous example the Condition is that one day has passed (Frequency). The Consequence is that 'Eating an Apple' is a Need again.

    The way we do that is by making this structure:
  ]

  #idea(
    [What is a Need?],
    visual-text: (
      "graph LR;
      A[Need] --> B[Action: Habit, Task]
      A[Need] --> C[Item: Food, Tool]
      A[Need] --> D[Goal: Milestone, Dream]
      ;"
    ),
  )[
  ]

  #idea(
    [What is a Need?],
    visual-text: (
      "graph LR;
      A[Need] --> B[Action: Eat an Apple]
      A[Need] --> C[Item: Apple]
      A[Need] --> D[Goal: Be someone that plants Apple trees]
      ;"
    ),
  )[
  ]

  #idea(
    [How do we meet those Needs with Lince?],
    visual-text: (
      "graph LR;
      A[Need] --> B[Record]
    ;"
    ),
  )[
  ]

  #idea(
    [How do we meet those Needs with Lince?],
    visual-text: (
      "graph RL;
      A[Record]
      A <-- B[Action: Eat an Apple]
      A <-- C[Item: Apple]
      A <-- D[Goal: Be someone that plants Apple trees]
    ;"
    ),
  )[ ]

  #idea(
    [How do we meet any Need?],
    visual-text: (
      "graph LR;
      Need <-- Contribution
    ;"
    ),
  )[
  ]

  #idea(
    [Two sides of meeting a Need],
    visual-text: (
      "graph LR;

      Need <-- Apple <-- Contribution
    ;"
    ),
  )[
  ]

  #idea(
    [Before the meeting of the Need],
    visual-text: (
      "graph LR;
      A[-1 Need] <-- Apple <-- B[1 Contribution]
    ;"
    ),
  )[
  ]

  #idea(
    [During the meeting of a Need],
    visual-text: (
      "graph LR;
      A[-1 + 1 Need] <-- Apple <-- B[1 - 1 Contribution]
    ;"
    ),
  )[
  ]

  #idea(
    [After the meeting of a Need],
    visual-text: (
      "graph LR;
      A[0 Need] <-- Apple <-- B[0 Contribution]
    ;"
    ),
  )[
  ]

  #idea(
    [They both look Lookalike],
    visual-text: (
      "graph LR;
      Need <--> Contribution
    ;"
    ),
  )[
  ]

  #idea(
    [They are both sides of the same coin],
    visual-text: (
      "graph LR;
      Need -> Record <- Contribution
    ;"
    ),
  )[
  ]

  #idea(
    [Record-centered Application],
    visual-text: (
      "graph RL;
      A[Record envolving Apples]
      A <-- B[Need: Eat an Apple]
      A <-- C[Need: Apple]
      A <-- D[Need: Be someone that plants Apple trees]
      E[Contribution: Apple] -> A
    ;"
    ),
  )[
  ]

  #idea(
    [How does a Record work?],
    visual-text: (
      "graph LR;
      A[Record] -> B[Quantity (Number)]
      A -> C[Head (Title)]
      A -> D[Body (Description)]
    ;"
    ),
  )[
  ]

  #idea(
    [Example of a Record],
    visual-text: (
      "graph LR;
      A[Record] -> B[Quantity (-1)]
      A -> C[Head (Apple)]
      A -> D[Body ()]
    ;"
    ),
  )[
  ]

  #idea(
    [Record as a Need and Contribution],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #major("Transactions", $"Credit" -> "Debit"$)

  #idea(
    [Money],
    visual-text: (
      "graph LR
      A[Person A] --> C[1 Money]
      D[2 Apple] <-- B[Person B]
      C <-> D

    ;"
    ),
  )[
  ]

  #idea(
    [Dreamer],
    visual-text: (
      "graph LR
       C[1 Money] <-- A[Person A]
      D[A Brazzillion Apples] <-- B[Person B]
      A <- D

    ;"
    ),
  )[
  ]

  #idea(
    [Inequality],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

  #idea(
    [],
    visual-text: (
      "graph LR;

    ;"
    ),
  )[
  ]

]


#if slides-mode [
  #slides-deck([First Steps], subtitle: [Newbies' Tutorial])[
    #content
  ]
] else [
  #book(
    title: "First Steps",
    subtitle: "Newbies' Tutorial",
    start-date: datetime(year: 2026, month: 3, day: 21),
    [
      #content
    ],
  )
]
