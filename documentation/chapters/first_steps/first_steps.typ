#import "../../components/chapter.typ": major
#import "../../components/components.typ": idea

#let lang = sys.inputs.at("lang", default: "pt")
#let tr(en, pt) = if lang == "pt" { pt } else { en }

#let content = [
  #major(tr("What is Lince?", "O que é Lince?"), $???$)

  #idea(
    [#tr(
      "Lince helps you meet any Need",
      "Lince ajuda você a atender qualquer Necessidade",
    )],
    visual-text: tr(
      "graph LR;
    A[What is a Need?];",
      "graph LR;
    A[O que é uma Necessidade?];",
    ),
  )[
  ]

  #idea(
    [#tr("What is a Need?", "O que é uma Necessidade?")],
    visual-text: tr(
      "graph LR;
      A[Need] --> B[Action: Habit, Task]
      A[Need] --> C[Item: Food, Tool]
      A[Need] --> D[Goal: Milestone, Dream]
      ;",
      "graph LR;
      A[Necessidade] --> B[Ação: Hábito, Tarefa]
      A[Necessidade] --> C[Item: Comida, Ferramenta]
      A[Necessidade] --> D[Objetivo: Marco, Sonho]
      ;",
    ),
  )[
  ]

  #idea(
    [#tr("What is a Need?", "O que é uma Necessidade?")],
    visual-text: tr(
      "graph LR;
      A[Need] --> B[Action: Eat an Apple]
      A[Need] --> C[Item: Apple]
      A[Need] --> D[Goal: Be someone that plants Apple trees]
      ;",
      "graph LR;
      A[Necessidade] --> B[Ação: Comer uma maçã]
      A[Necessidade] --> C[Item: Maçã]
      A[Necessidade] --> D[Objetivo: Ser alguém que planta macieiras]
      ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "How do we meet those Needs with Lince?",
      "Como atendemos essas Necessidades com Lince?",
    )],
    visual-text: tr(
      "graph LR;
      A[Need] --> B[Record]
    ;",
      "graph LR;
      A[Necessidade] --> B[Registro]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "How do we meet those Needs with Lince?",
      "Como atendemos essas Necessidades com Lince?",
    )],
    visual-text: tr(
      "graph RL;
      A[Record]
      A <-- B[Action: Eat an Apple]
      A <-- C[Item: Apple]
      A <-- D[Goal: Be someone that plants Apple trees]
    ;",
      "graph RL;
      A[Registro]
      A <-- B[Ação: Comer uma maçã]
      A <-- C[Item: Maçã]
      A <-- D[Objetivo: Ser alguém que planta macieiras]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("How do we meet any Need?", "Como atendemos qualquer Necessidade?")],
    visual-text: tr(
      "graph LR;
      Need <-- Contribution
    ;",
      "graph LR;
      Necessidade <-- Contribuição
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "Two sides of meeting a Need",
      "Dois lados de atender uma Necessidade",
    )],
    visual-text: tr(
      "graph LR;

      Need <-- Apple <-- Contribution
    ;",
      "graph LR;

      Necessidade <-- Maçã <-- Contribuição
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "Before the meeting of the Need",
      "Antes do atendimento da Necessidade",
    )],
    visual-text: tr(
      "graph LR;
      A[-1 Need] <-- Apple <-- B[1 Contribution]
    ;",
      "graph LR;
      A[-1 Necessidade] <-- Maçã <-- B[1 Contribuição]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "During the meeting of a Need",
      "Durante o atendimento de uma Necessidade",
    )],
    visual-text: tr(
      "graph LR;
      A[-1 + 1 Need] <-- Apple <-- B[1 - 1 Contribution]
    ;",
      "graph LR;
      A[-1 + 1 Necessidade] <-- Maçã <-- B[1 - 1 Contribuição]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "After the meeting of a Need",
      "Depois do atendimento de uma Necessidade",
    )],
    visual-text: tr(
      "graph LR;
      A[0 Need] <-- Apple <-- B[0 Contribution]
    ;",
      "graph LR;
      A[0 Necessidade] <-- Maçã <-- B[0 Contribuição]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("They both look Lookalike", "Ambos se parecem")],
    visual-text: tr(
      "graph LR;
      Need <--> Contribution
    ;",
      "graph LR;
      Necessidade <--> Contribuição
    ;",
    ),
  )[
  ]

  #idea(
    [#tr(
      "They are both sides of the same coin",
      "São os dois lados da mesma moeda",
    )],
    visual-text: tr(
      "graph LR;
      Need -> Record <- Contribution
    ;",
      "graph LR;
      Necessidade -> Registro <- Contribuição
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Record-centered Application", "Aplicação centrada em registros")],
    visual-text: tr(
      "graph RL;
      A[Record envolving Apples]
      A <-- B[Need: Eat an Apple]
      A <-- C[Need: Apple]
      A <-- D[Need: Be someone that plants Apple trees]
      E[Contribution: Apple] -> A
    ;",
      "graph RL;
      A[Registro envolvendo maçãs]
      A <-- B[Necessidade: Comer uma maçã]
      A <-- C[Necessidade: Maçã]
      A <-- D[Necessidade: Ser alguém que planta macieiras]
      E[Contribuição: Maçã] -> A
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("How does a Record work?", "Como um registro funciona?")],
    visual-text: tr(
      "graph LR;
      A[Record] -> B[Quantity (Number)]
      A -> C[Head (Title)]
      A -> D[Body (Description)]
      A -> E[*]
    ;",
      "graph LR;
      A[Registro] -> B[Quantidade (Número)]
      A -> C[Cabeçalho (Título)]
      A -> D[Corpo (Descrição)]
      A -> E[*]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Example of a Record", "Exemplo de um registro")],
    visual-text: tr(
      "graph LR;
      A[Record] -> B[Quantity (-1)]
      A -> C[Head (Apple)]
    ;",
      "graph LR;
      A[Registro] -> B[Quantidade (-1)]
      A -> C[Cabeçalho (Maçã)]
    ;",
    ),
  )[
  ]

  #major("Karma", $$)

  #idea(
    [#tr("If, and, then...", "Se, e, então...")],
    visual-text: tr(
      "graph LR;
      A[If: Condition] -> B[And: Threshold] -> C[Then: Consequence]
    ;",
      "graph LR;
      A[Se: Condição] -> B[E: Limiar] -> C[Então: Consequência]
    ;",
    ),
  )[]

  #idea(
    [#tr("If, and, then...", "Se, e, então...")],
    visual-text: tr(
      "graph LR;
      A[If: Tomorrow cold enough] -> B[And: Only if cold enough] -> C[Then: Need to pack sweater]
    ;",
      "graph LR;
      A[Se: Amanhã estiver frio o suficiente] -> B[E: Somente se estiver frio o suficiente] -> C[Então: Precisa levar um suéter]
    ;",
    ),
  )[]

  #idea(
    [#tr("If, and, then...", "Se, e, então...")],
    visual-text: tr(
      "graph LR;
      A[If: Tomorrow not cold enough] -> B[And: Only if cold enough] -> C[Then: No Need to pack sweater]
    ;",
      "graph LR;
      A[Se: Amanhã não estiver frio o suficiente] -> B[E: Somente se estiver frio o suficiente] -> C[Então: Não precisa levar um suéter]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph LR;
      A[Some Frequency]
      B[Computer (Shell) Command]
      C[A Certain Record's Quantity]
    ;",
      "graph LR;
      A[Alguma Frequência]
      B[Comando de computador (Shell)]
      C[Quantidade de um registro específico]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph LR;
      A[0 or 1]
      B[Number (or Text in future)]
      C[- Infinity to + Infinity]
    ;",
      "graph LR;
      A[0 ou 1]
      B[Número (ou texto no futuro)]
      C[- Infinito a + Infinito]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph TD;
      Data
    ;",
      "graph TD;
      Dados
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph LR;
      A[Condition: Data > 5]
    ;",
      "graph LR;
      A[Condição: Dados > 5]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
    ;",
      "graph TD;
      A[Condição: Dados > 5]
      B[Condição: Quantidade de um registro específico > 5]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
      C[Condition: 2 > 5]
    ;",
      "graph TD;
      A[Condição: Dados > 5]
      B[Condição: Quantidade de um registro específico > 5]
      C[Condição: 2 > 5]
    ;",
    ),
  )[]

  #idea(
    [#tr("\"If\" is the Condition", "\"Se\" é a Condição")],
    visual-text: tr(
      "graph TD;
      A[Condition: Data > 5]
      B[Condition: Certain Record's Quantity > 5]
      C[Condition: 2 > 5]
      D[Condition: False]
    ;",
      "graph TD;
      A[Condição: Dados > 5]
      B[Condição: Quantidade de um registro específico > 5]
      C[Condição: 2 > 5]
      D[Condição: Falso]
    ;",
    ),
  )[]

  #idea(
    [#tr("Two scenarios", "Dois cenários")],
    visual-text: tr(
      "graph TD;

      A[Condition: 0 (False)]
      B[Condition: 2 (Record Quantity)]
      ;",
      "graph TD;

      A[Condição: 0 (Falso)]
      B[Condição: 2 (Quantidade do registro)]
      ;",
    ),
  )[]

  #idea(
    [#tr("0 scenario", "Cenário 0")],
    visual-text: tr(
      "graph LR;

      A[Condition: 0 (False)] -> C[Threshold: Not 0]
    ;",
      "graph LR;

      A[Condição: 0 (Falso)] -> C[Limiar: diferente de 0]
    ;",
    ),
  )[]

  #idea(
    [#tr("2 scenario", "Cenário 2")],
    visual-text: tr(
      "graph LR;

      B[Condition: 2 (Record Quantity)] -> E[Threshold: Not 0] -> D[Consequence]
    ;",
      "graph LR;

      B[Condição: 2 (Quantidade do registro)] -> E[Limiar: diferente de 0] -> D[Consequência]
    ;",
    ),
  )[]

  #idea(
    [#tr("Con-se-quen-ces!", "Con-se-quên-cias!")],
    visual-text: tr(
      "graph LR;
      A[Record Changing]
      B[Shell Command]
      C[SQL]
    ;",
      "graph LR;
      A[Registro mudando]
      B[Comando Shell]
      C[SQL]
    ;",
    ),
  )[]

  #idea(
    [#tr("Cascating", "Encadeando")],
    visual-text: tr(
      "graph TD;
      A[Condition: Record reaches some value] -> B[Consequence: Record becomes new value] -> C[Condition: Record becomes new value]
    ;",
      "graph TD;
      A[Condição: o registro atinge algum valor] -> B[Consequência: o registro se torna um novo valor] -> C[Condição: o registro se torna um novo valor]
    ;",
    ),
  )[]

  #idea(
    [#tr("Cascatinging", "Cascateando de novo")],
    visual-text: tr(
      "graph TD;
      C[Condition: Record becomes new value] -> D[Command is run]
    ;",
      "graph TD;
      C[Condição: o registro se torna um novo valor] -> D[O comando é executado]
    ;",
    ),
  )[]

  #idea(
    [#tr("Practical example", "Exemplo prático")],
    visual-text: tr(
      "graph LR;
      A[-1 * (Frequency * Record Quantity + Command)] -> B[Threshold] -> C[Record's Quantity]

    ;",
      "graph LR;
      A[-1 * (Frequência * Quantidade do registro + Comando)] -> B[Limiar] -> C[Quantidade do registro]

    ;",
    ),
  )[]

  #idea(
    [#tr("Flow", "Fluxo")],
    visual-text: tr(
      "graph LR;
      A[Frequency] -> B[Condition]
      C[Command] -> B
      D[SQL] -> B
      E[Record] -> B
      B -> F[Threshold]
      F -> G[Record]
      F -> H[Command]
      F -> I[SQL]

    ;",
      "graph LR;
      A[Frequência] -> B[Condição]
      C[Comando] -> B
      D[SQL] -> B
      E[Registro] -> B
      B -> F[Limiar]
      F -> G[Registro]
      F -> H[Comando]
      F -> I[SQL]

    ;",
    ),
  )[]

  #major(tr("Transactions", "Transações"), tr(
    $"Credit" -> "Debit"$,
    $"Crédito" -> "Débito"$,
  ))

  #idea(
    [#tr("Money", "Dinheiro")],
    visual-text: tr(
      "graph LR
      A[Person A] --> C[1 Money]
      D[2 Apple] <-- B[Person B]
      C <-> D

    ;",
      "graph LR
      A[Pessoa A] --> C[1 Dinheiro]
      D[2 Maçãs] <-- B[Pessoa B]
      C <-> D

    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Dreamer", "Sonhador")],
    visual-text: tr(
      "graph LR
       C[1 Money] <-- A[Person A]
      D[A Brazzillion Apples] <-- B[Person B]
      A <- D
    ;",
      "graph LR
       C[1 Dinheiro] <-- A[Pessoa A]
      D[Um brazilhão de maçãs] <-- B[Pessoa B]
      A <- D
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Three-party transaction", "Transação entre três partes")],
    visual-text: tr(
      "graph LR;
      A -> B
      B -> C
      C -> A
    ;",
      "graph LR;
      A -> B
      B -> C
      C -> A
    ;",
    ),
  )[]

  #major("Lince", $$)

  #idea(
    [#tr("You", "Você")],
    visual-text: tr(
      "graph LR;
      Cell
    ;",
      "graph LR;
      Célula
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("You and others", "Você e outras pessoas")],
    visual-text: tr(
      "graph LR;
      A[Cell]
      B[Cell]
      C[Cell]
      D[Cell]
    ;",
      "graph LR;
      A[Célula]
      B[Célula]
      C[Célula]
      D[Célula]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Organ", "Órgão")],
    visual-text: tr(
      "graph TD;
      O[Organ]
     O[Organ] <-- A[Cell]
     O[Organ] <-- B[Cell]
     O[Organ] <-- C[Cell]
     O[Organ] <-- D[Cell]
    ;",
      "graph TD;
      O[Órgão]
     O[Órgão] <-- A[Célula]
     O[Órgão] <-- B[Célula]
     O[Órgão] <-- C[Célula]
     O[Órgão] <-- D[Célula]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Organs", "Órgãos")],
    visual-text: tr(
      "graph TD;
      B[Work] <-> A[Your Cell] <-> C[Hobby]

      D[Family] <-> A <-> E[Friends]
    ;",
      "graph TD;
      B[Trabalho] <-> A[Sua célula] <-> C[Passatempo]

      D[Família] <-> A <-> E[Amigos]
    ;",
    ),
  )[
  ]

  #idea(
    [#tr("Lince", "Lince")],
    visual-text: tr(
      "graph TD;
     O[Lince] <-- A[Organ]
     O[Lince] <-- B[Cell]
     O[Lince] <-- C[Organ]
     O[Lince] <-- D[Cell]
    ;",
      "graph TD;
     O[Lince] <-- A[Órgão]
     O[Lince] <-- B[Célula]
     O[Lince] <-- C[Órgão]
     O[Lince] <-- D[Célula]
    ;",
    ),
  )[
  ]
]

#major(
  tr("First Steps", "Primeiros Passos"),
  $$,
)

#content
