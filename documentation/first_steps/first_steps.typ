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
      A -> E[*]
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
    ;"
    ),
  )[
  ]
  #major("Karma", $$)

  #idea(
    [If, and, then...],
    visual-text: (
      "graph LR;
      A[If: Condition] -> B[And: Threshold] -> C[Then: Consequence]
    ;"
    ),
  )[]

  #idea(
    [If, and, then...],
    visual-text: (
      "graph LR;
      A[If: Tomorrow cold enough] -> B[And: Only if cold enough] -> C[Then: Need to pack sweater]
    ;"
    ),
  )[]

  #idea(
    [If, and, then...],
    visual-text: (
      "graph LR;
      A[If: Tomorrow not cold enough] -> B[And: Only if cold enough] -> C[Then: No Need to pack sweater]
    ;"
    ),
  )[]


  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph LR;
      A[Some Frequency]
      B[Computer (Shell) Command]
      C[A Certain Record's Quantity]
    ;"
    ),
  )[]

  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph LR;
      A[0 or 1]
      B[Number (or Text in future)]
      C[- Infinity to + Infinity]
    ;"
    ),
  )[]

  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph TD;
      Data
    ;"
    ),
  )[]

  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph LR;
      A[Condition: Data > 5]
    ;"
    ),
  )[]

  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
    ;"
    ),
  )[]

  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
      C[Condition: 2 > 5]
    ;"
    ),
  )[]


  #idea(
    [" If " is the Condition],
    visual-text: (
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
      C[Condition: 2 > 5]
      D[Condition: False]
    ;"
    ),
  )[]


  #idea(
    [Two scenarios],
    visual-text: (
      "graph TD;

      A[Condition: 0 (False)]
      B[Condition: 2 (Record Quantity)]
      ;"
    ),
  )[]

  #idea(
    [0 scenario],
    visual-text: (
      "graph LR;

      A[Condition: 0 (False)] -> C[Threshold: Not 0]
    ;"
    ),
  )[]

  #idea(
    [2 scenario],
    visual-text: (
      "graph LR;

      B[Condition: 2 (Record Quantity)] -> E[Threshold: Not 0] -> D[Consequence]
    ;"
    ),
  )[]


  #idea(
    [Con-se-quen-ces!],
    visual-text: (
      "graph LR;
      A[Record Changing]
      B[Shell Command]
      C[SQL]
    ;"
    ),
  )[]

  #idea(
    [Cascating],
    visual-text: (
      "graph TD;
      A[Condition: Record reaches some value] -> B[Consequence: Record becomes new value] -> C[Condition: Record becomes new value]
    ;"
    ),
  )[]

  #idea(
    [Cascatinging],
    visual-text: (
      "graph TD;
      C[Condition: Record becomes new value] -> D[Command is run]
    ;"
    ),
  )[]

  #idea(
    [Practical example],
    visual-text: (
      "graph LR;
      A[-1 * (Frequency * Record Quantity + Command)] -> B[Threshold] -> C[Record's Quantity]

    ;"
    ),
  )[]

  #idea(
    [Flow],
    visual-text: (
      "graph LR;
      A[Frequency] -> B[Condition]
      C[Command] -> B
      D[SQL] -> B
      E[Record] -> B
      B -> F[Threshold]
      F -> G[Record]
      F -> H[Command]
      F -> I[SQL]

    ;"
    ),
  )[]

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
    [Three-party transaction],
    visual-text: (
      "graph LR;
      A -> B
      B -> C
      C -> A
    ;"
    ),
  )[]

  #major("Lince", $$)
  #idea(
    [You],
    visual-text: (
      "graph LR;
      Cell
    ;"
    ),
  )[
  ]

  #idea(
    [You and others],
    visual-text: (
      "graph LR;
      A[Cell]
      B[Cell]
      C[Cell]
      D[Cell]
    ;"
    ),
  )[
  ]

  #idea(
    [Organ],
    visual-text: (
      "graph TD;
      O[Organ]
     O[Organ] <-- A[Cell]
     O[Organ] <-- B[Cell]
     O[Organ] <-- C[Cell]
     O[Organ] <-- D[Cell]
    ;"
    ),
  )[
  ]

  #idea(
    [Organs],
    visual-text: (
      "graph TD;
      B[Work] <-> A[Your Cell] <-> C[Hobby]

      D[Family] <-> A <-> E[Friends]
    ;"
    ),
  )[
  ]

  #idea(
    [Lince],
    visual-text: (
      "graph TD;
     O[Lince] <-- A[Organ]
     O[Lince] <-- B[Cell]
     O[Lince] <-- C[Organ]
     O[Lince] <-- D[Cell]
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
