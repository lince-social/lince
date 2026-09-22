#set document(
  title: "Como proteger seu aplicativo no GitHub",
  author: "Guia para quem contrata desenvolvimento",
  description: "Guia ilustrado para proteger o código do aplicativo e do servidor, testar em staging e controlar a publicação.",
  date: datetime(year: 2026, month: 9, day: 21),
)
#let ink = rgb("17313C")
#let teal = rgb("12685C")
#let pale = rgb("EDF6F3")
#let gray = rgb("52636B")
#let edge = rgb("D2DFDF")
#set page(
  paper: "a4",
  margin: (top: 18mm, bottom: 19mm, x: 19mm),
  header: text(size: 8.5pt, fill: gray)[SEU PROJETO NO GITHUB · GUIA ILUSTRADO],
  footer: context [
    #line(length: 100%, stroke: 0.5pt + edge)
    #v(2mm)
    #text(size: 9pt, fill: gray)[Português brasileiro · 21/09/2026 #h(1fr) #counter(page).display("1 / 1", both: true)]
  ],
)
#set text(font: "Libertinus Serif", size: 12.5pt, fill: ink, lang: "pt", region: "BR")
#set par(leading: 0.55em, spacing: 0.7em)
#set heading(numbering: none)
#show heading.where(level: 1): set text(size: 26pt, weight: "bold")
#show heading.where(level: 2): set text(size: 16pt, weight: "bold", fill: teal)
#show link: set text(fill: teal)
#set list(indent: 12pt, body-indent: 6pt, spacing: 6pt)
#set enum(indent: 13pt, body-indent: 6pt, spacing: 7pt)
#set table(inset: 8pt, stroke: 0.5pt + edge)
#let sources = (
  ("Quem pode fazer o quê no projeto", "https://docs.github.com/pt/organizations/managing-user-access-to-your-organizations-repositories/managing-repository-roles/repository-roles-for-an-organization"),
  ("Proteção de branches e planos necessários", "https://docs.github.com/pt/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches"),
  ("Como configurar a proteção", "https://docs.github.com/pt/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/managing-a-branch-protection-rule"),
  ("Como criar uma branch", "https://docs.github.com/pt/pull-requests/how-tos/commit-changes/managing-branches-within-your-repository"),
  ("Como escolher a branch principal", "https://docs.github.com/pt/repositories/configuring-branches-and-merges-in-your-repository/managing-branches-in-your-repository/changing-the-default-branch"),
  ("Configurações dos ambientes e limites dos planos", "https://docs.github.com/pt/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments"),
  ("Como registrar uma aprovação", "https://docs.github.com/pt/pull-requests/how-tos/review-pull-requests/approving-a-pull-request-with-required-reviews"),
  ("Como aceitar a mudança na versão principal", "https://docs.github.com/pt/pull-requests/how-tos/merge-and-close-pull-requests/merging-a-pull-request"),
  ("Cópias de segurança do projeto", "https://docs.github.com/pt/repositories/archiving-a-github-repository/backing-up-a-repository"),
  ("Recuperação de um projeto apagado", "https://docs.github.com/pt/repositories/creating-and-managing-repositories/restoring-a-deleted-repository"),
  ("Segurança dos programas que executam tarefas automáticas", "https://docs.github.com/pt/actions/reference/security/secure-use"),
  ("Como gerenciar as pessoas com acesso", "https://docs.github.com/pt/repositories/managing-your-repositorys-settings-and-features/managing-repository-settings/managing-teams-and-people-with-access-to-your-repository"),
  ("Proteção do login em duas etapas", "https://docs.github.com/pt/organizations/keeping-your-organization-secure/managing-two-factor-authentication-for-your-organization/requiring-two-factor-authentication-in-your-organization"),
  ("App de testes e app de produção: exemplo oficial", "https://docs.flutter.dev/deployment/flavors"),
  ("Versões diferentes do mesmo app no Android", "https://developer.android.com/build/build-variants?hl=pt-br"),
  ("TestFlight: instalar uma versão de testes da Apple", "https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/"),
  ("Testes internos e fechados no Google Play", "https://support.google.com/googleplay/android-developer/answer/9845334?hl=pt-BR"),
  ("Liberação manual de uma versão na App Store", "https://developer.apple.com/help/app-store-connect/manage-your-apps-availability/select-an-app-store-version-release-option/"),
  ("Controle da publicação no Google Play", "https://support.google.com/googleplay/android-developer/answer/9859654?hl=pt-BR"),
  ("Separação dos ambientes no Firebase", "https://firebase.google.com/docs/projects/dev-workflows/general-best-practices?hl=pt-br"),
)
#let cite(n) = box(text(size: 9pt)[#link(sources.at(n - 1).at(1))[\[#n\]]])
#let callout(title, body) = block(width: 100%, inset: 11pt, radius: 4pt, fill: pale, stroke: 0.5pt + edge)[
  #text(weight: "bold", fill: teal)[#title] #linebreak()
  #body
]
#let check(body) = block(above: 6pt, below: 6pt)[
  #grid(columns: (10pt, 1fr), column-gutter: 6pt,
    [#box(width: 8pt, height: 8pt, stroke: 0.7pt + gray)], body)
]
#let step(n, title, body) = block(above: 10pt, below: 10pt)[
  #grid(columns: (23pt, 1fr), column-gutter: 7pt,
    text(size: 20pt, weight: "bold", fill: teal)[#n],
    [*#title* #linebreak() #body])
]
#let assets = json("images/sources.json")
#let photo(file, caption, width: 100%, credit: "GitHub") = block(above: 8pt, below: 10pt, breakable: false)[
  #align(center)[#image("images/" + file, width: width, alt: caption)]
  #v(3pt)
  #text(size: 10pt, fill: gray)[#caption #link(assets.files.find(item => item.name == file).url)[Imagem: #credit.]]
]

= Como proteger seu aplicativo\ no GitHub
#text(size: 17pt, fill: gray)[Um guia para quem contrata programadores]

#v(3mm)
#callout("A regra principal", [
  O profissional faz a mudança. Você testa em uma versão do aplicativo ligada ao servidor de testes. Só depois aceita o código e autoriza a publicação para os clientes.
])

== Primeiro: o que é o GitHub?
É um serviço que guarda o código do aplicativo e do servidor, junto com o histórico das mudanças. Cada conjunto de arquivos é chamado de *repositório*. Ele deve ficar na conta da empresa e ser *privado*.

== Como o trabalho deve acontecer
#grid(
  columns: (1fr, 13pt, 1fr, 13pt, 1fr), align: center + horizon,
  callout("O profissional", [Faz a mudança\ em um rascunho.]), [→],
  callout("Você testa", [Usa o aplicativo\ de testes.]), [→],
  callout("Você libera", [A versão aprovada\ pode ir ao ar.]),
)

== Quatro coisas que precisam ficar com você
#check[O controle das contas, dos pagamentos e da recuperação de senha.]
#check[A decisão de aceitar mudanças na versão principal.]
#check[A decisão de publicar a nova versão para os clientes.]
#check[Cópias de segurança que o contratado não consiga apagar.]

== Você não precisa saber programar
O *aplicativo* é o que a pessoa instala. O *servidor* é a parte que recebe os pedidos do aplicativo e cuida dos dados. Faça a configuração com um técnico de confiança; no dia a dia, você testa o aplicativo e decide se aprova.

#callout("Isso evita perder tudo?", [
  Reduz muito o risco. Proteger o código e separar os testes ajuda a evitar problemas. Para recuperar o aplicativo, o servidor e os dados, mantenha também cópias de segurança fora do alcance do contratado.
])

#text(size: 10pt, fill: gray)[As imagens são de documentações oficiais. Os botões podem aparecer em inglês; as legendas explicam o que procurar. O exemplo considera um app para celular.]

#pagebreak()
= 1. A empresa precisa ter as chaves
*Faça uma vez, antes de liberar o acesso ao profissional.*

1. Use uma *organização*: é o espaço da empresa no GitHub. Você deve ser o proprietário, com seu próprio login. Se o projeto estiver na conta do contratado, peça uma cópia de segurança e a transferência para a empresa.
2. Guarde com você a hospedagem, as cópias de segurança e as contas de publicação: *Google Play Console* e/ou *Apple Developer / App Store Connect*. A hospedagem mantém o servidor funcionando. As contas das lojas devem pertencer à empresa.
3. Ative o login em duas etapas, com uma confirmação além da senha, e guarde os códigos de recuperação. Exija isso dos participantes também.~#cite(13)

== Dê acesso para trabalhar, sem entregar a conta
Em cada repositório, abra *Settings* (configurações) e a área de pessoas com acesso. Use *Add people* para convidar o profissional. Escolha *Write*, que permite trabalhar no código. Reserve *Admin* e *Owner*, que dão controle amplo, para você.~#cite(1)

#photo("manage-access-overview.png", "Add people adiciona uma pessoa. Role mostra o nível de acesso. Para o contratado, escolha Write. Os nomes da imagem são exemplos.", width: 86%)

#callout("Confira com quem configura", [
  Os dois repositórios estão privados. O acesso padrão da organização está em *None* (nenhum). O contratado não recebe acesso maior por outra equipe. Cada pessoa usa sua própria conta.~#cite(12)
])

#text(size: 11pt)[Se você escolher outro administrador de confiança, ele também terá poder para mudar as proteções. Se quiser que só você tenha esse poder, mantenha apenas você como administrador e proprietário.]

#pagebreak()
= 2. Organize o aplicativo e o servidor
*Dois repositórios são uma opção, não uma exigência.* Neste guia, vamos usar um chamado *aplicativo* e outro chamado *servidor*. Se o código já fica junto, os mesmos cuidados podem ser aplicados sem separá-lo.

Cada repositório terá duas *branches*: linhas de trabalho do código. A `main` guarda o que você aprovou. A `staging` recebe o que será testado.

#table(
  columns: (27%, 36%, 37%),
  [*Repositório*], [*main*], [*staging*],
  [*aplicativo*], [Código aprovado do app], [Código do app de testes],
  [*servidor*], [Código aprovado do servidor], [Código do servidor de testes],
)

== Prepare as branches nos dois repositórios
Faça uma cópia de segurança. Peça ao técnico para colocar o código aprovado em `main`, preservando o histórico. Em *Settings → General*, escolha `main` como padrão (*Default branch*). Repita nos dois repositórios.~#cite(5)

== Crie staging a partir da main
#grid(columns: (1fr, 80mm), column-gutter: 12pt,
  [
    1. Abra a página do projeto e selecione `main` no botão de branches.
    2. Abra o mesmo botão e digite `staging` no campo de busca.
    3. Clique em *Create branch* para criar a versão de testes.~#cite(4)

    Na imagem, onde aparece *new-branch*, você deve digitar *staging*.
  ],
  [#photo("create-branch-text.png", "O novo trabalho parte da main. A imagem usa um nome de exemplo.")],
)

#callout("Repositório e ambiente são coisas diferentes", [
  Os repositórios separam o código do app e do servidor. Os ambientes separam os testes dos clientes. Não é preciso copiar todo o projeto para um repositório de “testes” e outro de “produção”.
])

== O profissional trabalha primeiro em um rascunho
Ele cria um rascunho e pede para levar a mudança à `staging`. Esse pedido se chama *pull request*, ou *PR*. Depois de testar, você aceita o pedido de `staging` para `main` em cada repositório que mudou.

#pagebreak()
= 3. Deixe só você aceitar mudanças
*Esta página é para configurar junto com o responsável técnico.* Para um projeto privado dentro de uma organização, use o plano *GitHub Team ou Enterprise*. O plano gratuito da organização não aplica as proteções descritas aqui.~#cite(2)

#photo("repo-actions-settings.png", "Na página do projeto, Settings abre as configurações.")

*Repita nos dois repositórios:* abra *Settings → Branches* e *Add classic branch protection rule*. No campo *Branch name pattern*, escreva `main`. Configure e salve a regra abaixo.~#cite(3)

#set table(inset: 7pt)
#table(
  columns: (68%, 32%),
  fill: (x, y) => if y == 0 { pale } else { none },
  table.header([*O que pedir ao responsável técnico*], [*Como deixar*]),
  [Toda mudança precisa passar por um pedido (PR).], [Ligado],
  [O pedido precisa de pelo menos uma aprovação.], [1 aprovação],
  [Mudou o código? A aprovação anterior perde a validade.], [Ligado],
  [Os testes automáticos precisam passar.], [Ligado; selecionar os testes reais],
  [O pedido deve incluir a main atual e resolver as pendências da revisão.], [Ligado],
  [Limitar quem pode alterar a main: *Restrict who can push to matching branches*.], [Só a sua conta],
  [Aplicar as regras também aos administradores: *Do not allow bypassing the above settings*.], [Ligado],
  [Permitir pular o pedido de mudança.], [Ninguém],
  [Permitir descartar uma revisão.], [Só você],
  [Permitir apagar a branch ou reescrever o histórico à força.], [Ambos desligados],
)

O responsável deve criar e executar os testes antes de selecioná-los como obrigatórios. Peça também para desligar a aceitação automática de pedidos (*auto-merge*) e conferir se há outras regras interferindo.~#cite(3)

#callout("Apenas exigir uma aprovação não basta", [
  A restrição de quem altera a `main` é o que reserva a decisão final para você. Não coloque o contratado nem programas automáticos nessa lista. Administradores continuam tendo poder para mudar as regras.~#cite(2)
])

Na `staging`, também exija pedidos e testes e impeça apagar a branch ou forçar mudanças no histórico. O contratado pode aceitar pedidos nela. Faça um teste com a conta dele: deve conseguir trabalhar na `staging`, mas não aceitar pedidos na `main`, mesmo com sua aprovação.

#pagebreak()
= 4. Separe os testes dos clientes
*Sim, este é um fluxo comum para aplicativos.* A documentação do Flutter mostra versões de testes e de produção, inclusive com endereços de servidor diferentes. É um exemplo dessa prática; seu app não precisa usar Flutter.~#cite(14)

#grid(columns: (1fr, 1fr), column-gutter: 12pt,
  callout("VOCÊ TESTA", [
    *App de testes*\
    ↓\
    *Servidor de testes — staging*\
    ↓\
    *Dados fictícios e contas de teste*
  ]),
  callout("CLIENTES USAM", [
    *App público*\
    ↓\
    *Servidor de produção*\
    ↓\
    *Dados reais dos clientes*
  ]),
)

== O que pedir ao responsável técnico
1. *Configure o endereço dentro de cada versão do app.* O app de testes só conversa com staging. O app público só conversa com produção. Nenhuma das versões troca de ambiente sozinha quando há erro.
2. *Separe os serviços e os dados.* Use servidores ou projetos isolados, contas e senhas diferentes. Staging não pode acessar dados nem backups de produção. Se usar Firebase, crie projetos separados por ambiente.~#cite(20)
3. *Separe pagamentos e mensagens.* Use cobranças fictícias. E-mails e notificações dos testes não devem chegar aos clientes.
4. *Faça o código certo chegar ao lugar certo.* A `staging` de cada repositório alimenta os testes. A `main` fornece o código aprovado para produção. Aceitar um pedido no GitHub não deve publicar automaticamente para os clientes.
5. *Guarde o controle da publicação.* Só você libera o servidor e o app público. Revise permissões na hospedagem, nas lojas e nos programas automáticos. Proteja também as chaves e certificados usados para publicar o app.

#callout("Teste sem colocar as senhas reais em risco", [
  O app instalado não deve conter senhas de administrador. Os programas que executam código ainda não aprovado não podem receber credenciais de produção. A área geral de “secrets” do GitHub não protege uma senha de quem pode mudar esses programas.~#cite(11)
])

== Se a publicação usar GitHub Actions
É o recurso de tarefas automáticas. Em cada repositório, o técnico deve restringir o ambiente `production` à branch `main`, guardar nele as credenciais e publicar apenas a versão aprovada.~#cite(6)

#text(size: 11.5pt)[*No GitHub Team*, projetos privados não têm a aprovação manual de publicação do próprio GitHub. Use a liberação na hospedagem e nas lojas, com acessos sob seu controle. O técnico deve demonstrar que o contratado não consegue contornar essa decisão.~#cite(6)]

#pagebreak()
= 5. Instale o aplicativo de testes
Você deve experimentar a mudança *no aplicativo instalado*, usando o servidor staging. Abrir apenas uma página do servidor não testa a experiência do usuário.

#grid(columns: (1fr, 72mm), column-gutter: 13pt,
  [
    == Evite confundir as versões
    Peça o nome *“Meu App — TESTE”*, um ícone diferente e a indicação *TESTES* dentro do aplicativo.

    Se quiser manter as duas versões instaladas juntas, o técnico deve configurar identificadores diferentes para elas. Isso não exige outro repositório.~#cite(15)

    A imagem mostra um exemplo oficial com dois aplicativos: produção e staging. Os nomes aparecem abreviados na tela.~#cite(14)
  ],
  [#photo("flutter-test-production-apps.png", "Na última linha, Flavors p… e Flavors s… são as duas versões do mesmo exemplo.", credit: "Flutter / Google")],
)

== Como receber a versão de testes
- *Se for para iPhone:* o responsável envia um convite pelo *TestFlight*. Você instala o TestFlight, aceita o convite e instala o app de testes.~#cite(16)
- *Se for para Android:* o responsável envia um convite de *teste interno ou fechado do Google Play*. Você entra com a conta convidada e segue o link de instalação.~#cite(17)

#callout("O convite não escolhe o servidor", [
  TestFlight e Google Play distribuem o arquivo do app. É o técnico quem configura sua ligação com staging. Peça uma demonstração: uma conta ou registro criado no teste deve aparecer apenas nos dados de teste.
])

== Deixe a publicação pública nas suas mãos
Na Apple, peça a *liberação manual* da versão na App Store.~#cite(18) No Google Play, use *Publicação Gerenciada* quando disponível; ela não cobre o primeiro lançamento nem as atualizações de teste interno.~#cite(19)

#text(size: 11pt)[Esses botões não substituem as permissões. O contratado não deve poder liberar o app público, mudar a regra ou usar uma chave de publicação para contorná-la. Mantenha na empresa o controle das contas e da recuperação.]

#pagebreak()
= 6. Teste, aprove e publique
*Repita para cada entrega. Registre o número do app de testes e a versão do servidor staging que foram usados juntos.*

#step("1", "Receba a entrega para testar.", [
  Peça a lista do que mudou e instale o app de testes. O profissional prepara um pedido de `staging` para `main` em cada repositório alterado. Se só o servidor mudou, teste-o com o app correspondente.
])

#callout("Antes de aprovar, confira", [
  #check[A mudança pedida funciona, inclusive quando você erra um campo.]
  #check[Login, busca, cadastro e outras tarefas antigas continuam funcionando.]
  #check[Nos tipos de celular atendidos, funcionam telas, câmera e notificações usadas pelo app.]
  #check[Os dados ficam em staging, sem cobranças reais nem acesso a dados de outra pessoa.]
])

#step("2", "Registre sua aprovação no pedido.", [
  Em cada pedido, confira destino *main* e origem *staging*. Abra *Files changed* (arquivos alterados) e *Review changes* (revisar). Anote as versões do app e do servidor. Marque *Approve* (aprovar) e *Submit review* (enviar revisão).~#cite(7)
])

#photo("review-changes-button.png", "Review changes abre a revisão. No seu pedido de entrega, a origem deve ser staging.")

#grid(columns: (1fr, 88mm), column-gutter: 12pt,
  [
    #step("3", "Aceite o código aprovado.", [
      Com os testes aprovados, volte à conversa. Clique em *Merge pull request* e em *Confirm merge*. Repita no outro repositório, se ele mudou.~#cite(8)
    ])
    Aceitar o código não libera o servidor nem o aplicativo para os clientes.
  ],
  [#photo("merge-pull-request-options.png", "Merge aceita a mudança na main. Use a opção Create a merge commit preparada pelo técnico.")],
)

*Antes de publicar:* o técnico confere o código final aprovado e valida o pacote público do app, configurado para produção. Não envie às lojas o pacote que aponta para staging. Se algo mudou, teste novamente. Em mudanças sensíveis, peça revisão de outro técnico.

*Você libera as duas partes:* combine a ordem da atualização do servidor e do app nas lojas. O servidor deve atender também o app que já está instalado enquanto a atualização chega. Confira o resultado; depois, o técnico atualiza as branches de testes para a próxima entrega.

#pagebreak()
= 7. Guarde uma saída de emergência
*Backup* quer dizer cópia de segurança. Ela deve permitir recuperar o projeto sem depender do contratado.

== Peça para configurar
#check[*Uma cópia completa dos dois repositórios e dos históricos*, incluindo branches, marcações de versões e arquivos grandes. Baixar só os arquivos atuais não guarda todo o histórico.~#cite(9)]
#check[*Cópias dos dados e arquivos dos clientes.* Guarde também configurações, pacotes publicados do app e meios de recuperar as contas e chaves de publicação.]
#check[*Um local sob seu controle*, fora da conta de trabalho do contratado. Use cópias protegidas por criptografia e guarde as chaves de recuperação. Nem o contratado nem seus programas devem conseguir apagar as cópias antigas.]
#check[*Cópias de várias datas.* Comece com cópias diárias e antes de publicar mudanças. Guarde, por exemplo, 90 dias; aumente a frequência se perder um dia de trabalho for inaceitável. Mantenha outra cópia em outro serviço ou desconectada.]
#check[*Um teste de recuperação agora e a cada três meses.* Peça para restaurar em local separado e mostrar o app funcionando com o servidor e os dados recuperados. Receba avisos se uma cópia falhar.]

#callout("Não vale como única cópia", [
  Outra branch dentro do mesmo projeto, uma pasta no servidor do contratado ou uma cópia que simplesmente repete tudo o que foi apagado. É preciso conseguir voltar a uma data anterior.
])

== Se o projeto for apagado no GitHub
Peça ajuda imediatamente. Como proprietário, abra as configurações da organização e procure *Deleted repositories* (projetos apagados). Para projetos que atendem às condições do GitHub, há recuperação por até 90 dias. Existem exceções; conte também com suas próprias cópias.~#cite(10)

#photo("restore-button.png", "Restore significa restaurar. Use essa opção se o projeto aparecer na lista de apagados.")

== Se uma atualização der problema
Pause a distribuição. O servidor pode voltar a uma versão anterior, se for seguro para os dados. O app já instalado no celular não volta automaticamente: pode ser necessário publicar uma correção. Recuperar o código também não recupera dados apagados.

== Quando o contrato terminar
Remova acessos ao GitHub, à hospedagem e às lojas. Revise chaves e certificados com o técnico e troque senhas compartilhadas. Confira os backups. Remover a pessoa do GitHub não cancela toda chave de acesso nem apaga cópias que ela já baixou.~#cite(1)

#pagebreak()
= 8. Confira antes de considerar pronto
Peça uma demonstração. Use uma conta de contratado para conferir as restrições. Testes de apagar ou forçar mudanças devem acontecer só em uma cópia descartável.

#check[Consigo entrar nas contas e recuperá-las sem ajuda do contratado.]
#check[Os dois repositórios são privados e o plano aplica as proteções.]
#check[O contratado trabalha, mas não aceita mudanças na main de nenhum deles.]
#check[Se um teste falha ou o código muda, a aprovação antiga não basta.]
#check[O app de testes aponta para staging e não acessa os dados dos clientes.]
#check[Só eu libero o servidor e o app público; sei quais versões foram testadas juntas.]
#check[Vi uma cópia ser recuperada e sei quem chamar em caso de problema.]

#table(columns: (40%, 60%),
  [*Links: aplicativo e servidor*], [],
  [*Convite para instalar o app de testes*], [],
  [*Onde ficam as cópias*], [],
  [*Contato para recuperação*], [],
  [*Data do teste de recuperação*], [],
)

== Fontes e imagens
#text(size: 10.5pt)[Fontes oficiais consultadas em 21/09/2026. Links e créditos são clicáveis. As telas podem mudar. Dois repositórios e branches staging são a organização sugerida neste guia; outras equipes podem organizar o código de outra forma.]

#block[
  #set par(leading: 0.35em, spacing: 0.25em)
  #for (i, source) in sources.enumerate() [
    #text(size: 10pt)[#link(source.at(1))[#(i + 1). #source.at(0)]] #linebreak()
  ]
]

#text(size: 10pt, fill: gray)[
  Capturas: GitHub, Inc. e colaboradores de #link("https://github.com/github/docs")[github/docs]; imagem dos dois apps: #link("https://docs.flutter.dev/deployment/flavors")[Flutter / Google e colaboradores]. Licença #link("https://creativecommons.org/licenses/by/4.0/")[CC BY 4.0]. Imagens originais, apenas redimensionadas, com legendas acrescentadas. São exemplos públicos, não capturas da sua conta. Os desenhos explicativos foram feitos para este guia.
]

#text(size: 10pt, fill: gray)[Este material é um guia. A configuração do projeto, da hospedagem e das cópias deve ser feita e conferida com o responsável técnico.]
