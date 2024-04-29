# O que é a Lince?
A lince é uma ferramenta para cadastro e conexão entre necessidades e contribuições de escopo livre.

A documentação é este README.md. As tarefas estão no <a href="https://github.com/orgs/lince-social/projects/6">Kanban</a>. E-mail para contato: [xaviduds@gmail.com](mailto:xaviduds@gmail.com).

A lince é uma iniciativa sem fins lucrativos. Com o intuito de remunerar desenvolvedores, utiliza-se de financiamento popular, através de: [apoia.se/lince](https://www.apoia.se/lince), [github sponsors](https://github.com/sponsors/lince-social) e [patreon](https://www.patreon.com/lince_social).

# Instalação:
Utiliza-se PostgreSQL e <a href="https://nix.dev/install-nix.html">Nix</a> para criar os ambientes de execução de código. Verifique que estão instalados.

```bash
# Clone e entre no repositório:
git clone git@github.com:lince-social/lince.git && cd lince

# Inicie o ambiente:
nix flake update && nix develop

# Rode a Lince:
streamlit run src/lince.py
```

# Teoria:

Pode ser perspicaz acompanhar essa parte com o código da <a href="https://github.com/lince-social/lince/blob/main/src/postgre.sql">base_de_dados.sql</a> ao lado.

A lince é focada em cadastros e operações sobre esses cadastros, para começar a explicar tal mecanismo entraremos em um consenso do significante de Necessidades e Contribuições. Após isso será comentado sobre como imagina-se a tecnologia de uma Lince funcional. E por final o impacto imaginado com tal tecnologia.

# Necessidades:
Temos necessidades diferentes, podendo ser:

- Básicas e pessoais, como: água, luz, saneamento, saúde, educação, habitação, alimentação, roupas, hábitos, natureza, comunidade, etc.
- Serviços, como: transporte, apoio técnico, enfermagem, jardinagem, cultura, entretenimento, turismo, automatização, etc.
- Outros bens de consumo, como: eletrodomésticos, ferramentas de trabalho, internet, computação, máquinas e matéria prima, etc.

# Contribuições:
Assim como diversas formas de saciar tais necessidades, de forma individual ou através de organizações, contribuindo com:

 - Produtos.
 - Serviços, dentre eles: aulas, atendimento técnico, instalações, desenvolvimento, consultoria, etc.
 - Assistência social, trabalho comunitário, itens para doação, etc.
 - Informação, infraestrutura, financiamento, presença, etc.
 - Trabalho em geral.

# Tecnologia:
### Possibilidades:
Começa-se com software livre (FLOSS). Qualquer pessoa pode usar a Lince com o  código padrão ou criar sua versão, como customizações que ainda conseguem trocar informações. Estudam-se formas de autenticar identidades e transferir dados de forma descentralizada, com protocolos pub/sub, Merkle-CRDTs, bases de dados eventualmente consistentes e possívelmente blockchain, mas nada de cripto! 

### Lince padrão:
Pessoas poderão ter controle total dos seus dados, sendo o principal deles o 'cadastro'. Quando a quantidade do mesmo é positiva, ele é uma possível contribuição, caso negativa, é uma necessidade, se 0, não é contabilizado. Todas as outras funcionalidades tem como objetivo modificar essa quantidade.

O cadastro poderá ser feito por digitação, voz, foto ou vídeo, tornando-o acessível e fácil de usar. Para aqueles sem acesso à tecnologia, deseja-se ser possível adicionar suas necessidades e contribuições na Lince por meio de qualquer dispositivo.

Após cadastros serem criados, existe a possibilidade de troca, com 'proposta_transferência'. Caso deseje-se realizar uma troca de recursos, pode se colocar uma proposta de tal conteúdo: 
cadastro1 recebe tal quantidade de cadastro2, em troca aceito que cadastro3 envie tanto de quantidade para cadastro4. Exemplo: se você transferir uma quantidade de maçã para meu cadastro de maçã, transfiro tantos dinheiros do meu cadastro para o seu. Contribuição e Retribuição (opcional caso seja uma doação). 

Com o intuito de automatizar trabalho manual de modificar quantidade, é possível utilizar-se de uma 'observação_ponto'. Quanto um certo cadastro chegar a uma certa quantidade, a quantidade de um cadastro é alterada mais ou menos um valor, ou um script é rodado. Da mesma forma, pode-se realizar uma 'observacao_anicca', quando um cadastro modifica uma quantidade para mais ou para menos, outro cadastro tem seu valor modificado com uma proporção, podendo ser 1:1, 80:1, etc. Por fim existe a periodicidade, que com o passar de um tempo específico, também altera a quantidade do cadastro. 

No momento, utiliza-se Python pra a lógica e browser frontend e postgresql para o banco de dados.

# Impacto:
É importante destacar que a Lince é responsável por facilitar a conexão entre pessoas e recursos, transformando necessidades e contribuições em dados. Os custos relacionados à interação, como transporte, produção e serviços em si, continuam sendo responsabilidade das partes envolvidas.

Visando a maior flexibilidade de uso, a Lince consegue mapear e organizar diversas interações. Desde o fornecimento de apoio após desastres naturais até o planejamento de eventos, tarefas de organizações e pessoais, venda de produtos, etc. Queremos minimizar necessidades não atendidas, alinhar demandas e distribuições, e entender em que devemos trabalhar.

Muitas horas de trabalho humano são economizadas com o desenvolvimento tecnológico concentrado em um sistema. Tal concentração é possível com a flexibilidade e descentralização do aplicativo. Grandes empresas que não participam do processo operacional, mas recebem uma parte dos recursos de trabalhadores por oferecerem as plataformas deixarão de serem necessárias. Relações e fenômenos econômicos existentes poderão migrar para uma tecnologia com interações diretas entre pessoas caso desejado ou indiretas através de órgãos que centralizam e validam a identidade de profissionais de uma área.

Atualmente, grandes empresas e conglomerados tem a vantagem de poder desenvolver e dominar redes digitais e de suprimentos que as pessoas utilizam para trocar recursos. Com isso recebem uma a maior parcela possível dos rendimentos, participando o mínimo possível dos custos das operações. Com um aplicativo descentralizado, que permita o contato direto entre pessoas (desenvolvido por uma organização sem fins lucrativos que disponibiliza o código aberto) plataformas de Big Techs não serão mais necessárias e trabalhadores ficarão com uma parcela maior dos rendimentos.

Ao digitalizar as informações, a Lince possibilita o uso de algoritmos de otimização e aprendizado de máquina para um planejamento mais efetivo das contribuições. No entanto, é necessário considerar os vieses humanos presentes nos algoritmos e garantir transparência, consentimento e democracia ao implementar qualquer inteligência artificial tomadora de decisão.

A troca de informações sobre necessidades em tempo real tem o potencial de diminuir o "efeito chicote" nas redes de suprimentos. Esse efeito ocorre quando uma compra esperada não acontece, acumulando perdas e gerando ineficiências. Com um número significativo de usuários cadastrados as previsões de demanda se tornam desnecessárias. Caso a superprodução e perda de atendimento à demanda por informações incompletas e desatualizadas ocorra, ela pode ser mitigada com a consulta de cadastros Lince verdadeiros e atualizados. 

Mas o que fazer para incentivar tal forma de uso da tecnologia? É preciso desenvolver algo que faça sentido para as pessoas, algo que elas precisem, que as ajude na primeira interação delas, para construir de micro a macro impactos. 
