<h1 align="center"><img width=300px src="./media/logo/preto_no_branco.png"></h1>

Tool for registry and connection between Needs and Contributions with free scope.

<!-- # Showcase -->
<video controls src="media/showcase/current_version.mp4"></video>

# Table of Contents
<!-- - [Showcase](#showcase) -->
- [Installation](#installation)
- [Organization](#organization)
- [About](#about)
- [Disclamer](#disclamer)
- [License](#license)
- [Roadmap](#roadmap)

# Installation
Have <a href="https://nixos.org/download/">Nix</a> installed. After cloning the repo, to run Lince, just enter it and type:
```bash
nix-shell
```
# Organization
This is a non-profit project. Crowd funding is the source of development compensation: [apoia.se/lince](https://www.apoia.se/lince), [GitHub Sponsors](https://github.com/sponsors/lince-social) and [Patreon](https://www.patreon.com/lince_social).

# About
You can model goods and services with a Quantity, if it is below zero, it is a Need (N), if higher than zero, it is a Contribution (C). After the modeling is complete, it is possible to change the Quantity, be it through human trades or donations. Alternativelly the user can also automate it, with the app's several functionalities, to get the most value out of the technology. Below are a few examples of what can be modeled:

## Needs:
- Basic and personal, such as: water, electricity, sanitation, health, education, housing, food, clothing, habits, nature, community, etc.
- Services, such as: transportation, technical support, nursing, gardening, culture, entertainment, tourism, automation, etc.
- Other consumer goods, such as: appliances, work tools, internet, computing, machinery and raw materials, etc.

## Contributions:
- Products.
- Services, including: classes, technical support, installations, development, consulting, etc.
- Social assistance, community work, donation items, etc.
- Information, infrastructure, financing, presence, etc.

Today many apps are developed with a specific segment of human interaction in mind. This project's goal is to allow any type of human interaction that can be best managed by technology to exist without an extra need of app development. Many hours of human work are saved with the technological development concentrated in a single system. Such concentration is possible with the flexibility and decentralization of the application. Large companies that do not participate in the operational process, but receive a portion of the resources of workers for offering the platforms will no longer be necessary. Existing economic relations and phenomena may migrate to a technology with direct interactions between people if desired or indirect through agencies that centralize and validate the identity of professionals in an area.

Currently, large companies and conglomerates have the advantage of being able to develop and dominate digital and supply networks that people use to exchange resources. With this, they receive the largest possible share of the income, participating minimally in the costs of the operations. With a decentralized application, which allows direct contact between people (developed by a non-profit organization that makes the open source code available) Big Tech platforms will no longer be necessary and workers will keep a larger share of the income.

The exchange of information about needs in real time has the potential to reduce the "bullwhip effect" in supply networks. This effect occurs when an expected purchase does not happen, accumulating losses and generating inefficiencies. With a significant number of registered users, demand forecasts become unnecessary. If overproduction and failure to meet demand due to incomplete and outdated information occurs, Lince can be a solution, consulting registrations of needs in real time.

Aiming for greater flexibility of use, Lince can map and organize various interactions. From providing support after natural disasters to planning events, organizational and personal tasks, selling products, etc. We want to minimize unmet needs, align demands and distributions, and understand what we need to work on, and when we dont need to work anymore.

# Disclamer
It is important to highlight that Lince is responsible for facilitating the connection between people and resources, transforming needs and contributions into data. The gains and losses related to the interaction, such as transportation, production and services themselves, remain the responsibility of the parties involved.

# License
This project is licensed under the GNU GPLv3 license. See the [LICENSE](LICENSE) file for details.

# Roadmap
- [ ] Decentralization
    - [ ] Eventually consistent databases
    - [ ] pub/sub protocols, i.e. "Pessoas podem se inscrever com pub/sub com cada cadastro, referente ao assunto escolhido."
    - [ ] Merkle-CRDTs implementation
    - [ ] Authentication. # Check gajim for login inspiration.
    - [ ] Fazer com que as pessoas possam utilizar máquinas que tem controle para aliviar o tráfego em certos pontos da rede e permitir mais em geral. Não necessariamente as que elas possuem.
    - [ ] É possível utilizar IPFS e libp2p, diversos outras ferramentas pra auxiliar no processo de compartilhamento de cadastros, condições e transferências
- [ ] Conditions (The objective is to have generalized conditions and consequences, so anything can trigger anything else. A periodicity can run a script, a checkpoint can change a quantity through a proportion, etc.)
    - [ ] Periodicity, i.e. "Every two months and 4 weeks on a Thursday"
        - [ ] Alem de rodar pra sempre, ter a possibilidade de acabar depois de tantos períodos ou tempo.
    - [ ] Checkpoint, i.e. "When a quantity reaches 4"
    - [ ] Rate, i.e. "When a quantity changes in a certain rate (change/time)"
    - [ ] Proportion, i.e. "When a quantity changes a certain number"
- [ ] Consequences
    - [ ] Checkpoint, i.e "Set a quantity to a specific number"
    - [ ] Delta, i.e. "Set a quantity to more or less a number, -1, +4, etc."
    - [ ] Command, i.e. "Shell command, being able to trigger any script in any language, easy to do with nix-shells for dev envs"
UI/UX
- [ ] The registration can be done by typing, voice, photo or video, making it accessible and easy to use. For those without access to technology, it is possible to add their needs and contributions through any device or party.
- [ ] Transfer proposal and connection, i.e "A proposal of transfering a quantity from A to B, in return (or not) C receives some from D" "If you transfer an amount of apples to my apple registration, I will transfer so much money from my registration to yours. Contribution and Retribution (optional if it is a donation)."
    - [ ] Transferência múltipla, i.e "Entregar diversos itens por um só. Para comprar essa bota eu ofereço 20 reais e um candelabro".
Simulation
- [ ] Algorithm and/or ML for optimization of transfer quotas and cost-efficient connections. By digitizing the information, Lince enables the use of optimization algorithms and machine learning for more effective planning of contributions. However, it is necessary to consider the human biases present in the algorithms and ensure transparency, consent and democracy when implementing any decision-making artificial intelligence. Lince Modelo/Template.
Sugestão de Enriquecimento de cards, a pessoa coloca um prompt/foto do NI/CE e um MMLLM adiciona metadata, dps a pessoa confirma.
Copiar especificações de cadastros outros para poupar tempo na criação de novos.
Se uma pessoa vê um cadastro, seja N ou C, ela pode poupar tempo clicando num botão no frontend, isso copia certas propriedades daquele cadastro, com uma cópia espelhada do cadastro original com uma escolha de quantidade.

Exemplo: uma pessoa posta que pode contribuir com 5 maçãs, cada uma por R$ 1,00 (um real). Se eu achar isso um preço razoável eu clico no botão e a Lince preenche o titulo, descrição, quantidade, preço, etc. mas não posta o cadastro, fica só na interface. Cabe à pessoa que clicou completar o preenchimento da quantidade de maçãs que ela quer, nesse caso 3, e dai ela clica em um botão que cria o cadastro no banco de dados de vdd e faz a conexão se ela quiser. É como se ela pedisse empresado as especificações do cadastro pra poupar tempo.

Isso é bom se ela quiser contribuir ou pedir diretamente pra aquele cadastro lince, ou se quiser aquele item mas de qualquer pessoa, dai ela muda o preço e/ou quantidade e joga pro mundo. Ou se quiser criar cadastros parecidos, tipo de comprar um par de chinelo 34 e outro 39 da mesma cor e marca.

Da pra usar isso pra simular acontecimentos, tu copia varios cadastros e antes de clicar postar vê como a mudança de recursos aconteceria com eles.

Criações de Ns e Cs automáticas, segundo certas boas práticas de grupos ou preferências. Se entende como produzir algo, ou algo sobre produção e se modela qual a melhor maneira de fazer isso. Quais recursos precisam estar em qual lugar. Quem precisa contribuir com o que. Esses templates podem ser disponibilizados como arquiteturas. Pessoas se voluntariam a preencher certos campos e pegam parte dessa arquitetura como responsabilidade. Quanto todas as partes tiverem sido pegas e feitas aquela Grande Necessidade estará completa. Algo que se precisa mas faz parte de um esforço coletivo para conseguí-la.

É preciso aprender como funcionam diversas engrenagens e variáveis de processos produtivos e fazer com que a Lince consiga melhor organizar essa produção de forma ótima, a melhor, não importa o escopo.

