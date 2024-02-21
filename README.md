# Lince

# o que é a lince?

a lince é uma ferramenta para cadastro e conexão entre necessidades e contribuições de escopo livre.

você pode acessar a base de código no repositório <a href="https://github.com/lince-social/lince">lince</a>, a documentação em <a href="https://github.com/lince-social/.github/tree/main/docs/pt_br/documenta%c3%a7%c3%a3o">.github</a>, e as tarefas no <a href="https://github.com/orgs/lince-social/projects/6/views/1">kanban</a>. 

## Necessidades (Ns):
Temos necessidades diferentes, podendo ser:

- Básicas e pessoais, como: água, luz, saneamento, saúde, educação, habitação, alimentação, roupas, hábitos, natureza, comunidade, etc.
- Serviços, como: transporte, apoio técnico, enfermagem, jardinagem, cultura, entretenimento, turismo, etc.
- Outros bens de consumo, como: eletrodomésticos, ferramentas de trabalho, internet, computação, máquinas e matéria prima, etc.

## Contribuições (Cs):
Assim como diversas formas de saciar tais necessidades, de forma individual ou através de organizações, contribuindo com:

 - Produtos.
 - Serviços, dentre eles: aulas, atendimento técnico, instalações, desenvolvimento, consultoria, etc.
 - Assistência social, trabalho comunitário, itens para doação, etc.
 - Informação, infraestrutura, financiamento, presença, etc.
 - Trabalho em geral.

## Estrutura:

a lince é uma iniciativa sem fins lucrativos.

com o intuito de remunerar desenvolvedores, utiliza-se de financiamento popular, através de: [apoia.se/lince](https://www.apoia.se/lince), [github sponsors](https://github.com/sponsors/lince-social) e [patreon](https://www.patreon.com/lince_social).

e-mail para contato: [xaviduds@gmail.com](mailto:xaviduds@gmail.com).


Ferramenta para cadastro e conexão entre necessidades e contribuições de escopo livre.

Repositório para todo o código, a documentação geral e tecnológica pode ser acessada nesse 
[link](https://github.com/lince-social).

## Imaginação:
pessoas podem colocar suas ideias de desenvolvimento (backlog) como CIs e empresas clientes selecionarem quais vão ser desenvolvidas na sprint tal
A Lince permite a criação de uma tabela para coordenar Necessidades Internas (NIs). Com o aumento do número de pessoas uma rede de apoio é formada através de Contribuições Externas (CEs). Também possibilitando Contribuições Internas (CIs) para atender às Necessidades Externas (NEs).

Com a flexibilidade de permitir qualquer tipo de cadastro, a Lince abre um leque de possibilidades. Desde o fornecimento de apoio após desastres naturais até o planejamento de eventos. Idas ao supermercado são facilitadas com Linces de famílias sendo preenchidos por todos. Sinalizações de manutenção de infraestruturas públicas serão como votos para a solução de tais carências. É possível criar recorrências de NIs para mapear seus hábitos, deixando-os visíveis é possível delegar parte deles a outras pessoas através de CEs. Queremos minimizar necessidades não atendidas, alinhar demandas e distribuições, e entender em que devemos trabalhar.

A troca de informações sobre necessidades em tempo real tem o potencial de diminuir o "efeito chicote" nas redes de suprimentos. Esse efeito ocorre quando uma compra esperada não acontece, acumulando perdas e gerando ineficiências. Com um número significativo de usuários cadastrados, as previsões de demanda podem se tornar desnecessárias; juntamente de uma diminuição de superprodução e falta de atendimento por informações de mercado incompletas.

A criação de uma plataforma para ofertar quaisquer produtos e serviços removerá barreiras para pequenos empreendedores e autônomos. Trabalho de qualquer tipo pode ser ofertado. Uma empresa pode criar NIs de tarefas a serem cumpridas e funcionários de insumos e ferramentas a serem comprados. Fluxos de atividades no trabalho podem ser acompanhados pela Lince. Além disso, pessoas e organizações que buscam trabalho comunitário poderão entrar em contato facilmente com aqueles que desejam impactar. Utilizamos recursos existentes, evitamos desperdícios, recebemos ajuda de outros, contribuímos por um preço e quando possível, gratuitamente.

A Lince estará disponível como um site ou aplicativo, sendo acessível em qualquer rede. Os usuários poderão participar de diversos círculos da Lince: mundiais, nacionais, empresariais, municipais, familiares, entre outros. Além disso, a plataforma permitirá que os usuários divulguem suas necessidades, ajudas e doações de forma anônima ou identificada, para todos ou apenas para aqueles que desejarem.

O projeto é uma iniciativa de software livre, permitindo que qualquer pessoa interessada possa contribuir para sua implementação e melhoria contínua. O cadastro poderá ser feito por digitação, voz, foto ou vídeo, tornando-o acessível e fácil de usar. Para aqueles sem acesso à tecnologia, será possível adicionar suas necessidades e capacidades na Lince por meio de instituições credenciadas, como o SUS (Sistema Único de Saúde).

A plataforma busca unificar as interações econômicas e as trocas de recursos entre pessoas, empresas, governos e ONGs, reduzindo a necessidade de utilizar diversos aplicativos. Empresas podem ter perfis oficiais na Lince, oferecendo seus produtos em vez de criarem um aplicativo próprio. O investimento privado no desenvolvimento será compartilhado, reduzindo custos e impulsionando o projeto.

É importante destacar que a Lince é responsável por facilitar a conexão entre pessoas e recursos, transformando necessidades e contribuições em dados. Os custos relacionados à interação, como transporte, produção e serviços em si, continuam sendo responsabilidade das partes envolvidas.

Ao digitalizar as informações, a Lince possibilita o uso de algoritmos de otimização e aprendizado de máquina para um planejamento mais efetivo das contribuições. No entanto, é necessário considerar os vieses humanos presentes nos algoritmos e garantir transparência, consentimento e democracia ao implementar qualquer inteligência artificial tomadora de decisão. A distribuição dos recursos deve ser avaliada cuidadosamente, levando em consideração critérios como proximidade, necessidade absoluta ou potencial de mudança social ao recebê-los.

## Instalação (ainda nao dockerizada, utilizando arch linux):
1. Instalar postgresql
2. Criar um banco de dados chamado personallince (não é necessário, mas o streamlit_crud.py tem esse nome de banco de dados como alvo de alterações então esse nome é pra dar match)
3. Rodar o script base_de_dados.sql enquanto no banco de dados personallince (\i [caminho pro script {no meu caso o comando é \i ~/lince/lince/base_de_dados.sql}])
4. Criar um ambiente virtual (boas práticas) (ex: conda create --name lince) e ativá-lo (conda activate lince)
5. Instalar, preferivelmente dentro do ambiente virtual, os pacotes de python pra rodar o streamlit_crud.py: pip install streamlit psycopg2 pandas uuid
6. Terminal: python streamlit_crud.py && streamlit run streamlit_crud.py
7. Provavelmente uma aba será aberta no broswer no localhost port 8501

