# Sand Class B Spec: Kanban de Record

## Objetivo

Este documento especifica o Kanban oficial recomendado para Lince Web usando `Sand Class B: Hybrid Reactive`.

Ele serve como uma especialização concreta do documento `web_sand_classes.md`.

Este Kanban é:

- orientado a `Record`
- alimentado por uma `View SSE`
- persistido por card em `.config/lince/web/board-state.json`
- autenticado pelo login geral da web
- híbrido entre Maud, Datastar e JavaScript puro

## Classe Escolhida

### Sand Class B: Hybrid Reactive

Esta é a melhor classe para este Kanban porque o componente precisa ao mesmo tempo de:

- estrutura rica e variável vinda do servidor
- estado de interface persistido por card
- interações locais complexas
- drag and drop
- resize de colunas
- múltiplos estados visuais
- feedback claro de conexão SSE e autenticação

`Sand Class A` seria forte demais no lado servidor e fraca demais para as interações locais.
`Sand Class C` ficaria boa demais só para shells estáveis e ruim para cards e colunas mutáveis.
`Sand Class D` colocaria JavaScript demais como default, o que não é o melhor ponto de equilíbrio para um Kanban oficial.

## Perfil Arquitetural

### Distribuição de responsabilidade

- Maud: 60%
- Datastar: 30%
- JavaScript puro: 10%

### Distribuição de updates SSE

- HTML fragments: 60%
- signal patches: 40%

Isso quer dizer:

- Maud é a fonte de verdade da estrutura
- Datastar é a fonte de verdade da UI local e do estado reativo do widget
- JavaScript puro fica restrito às interações de navegador que merecem controle imperativo

## Modelo de Estado

O componente deve separar estado em três grupos.

### 1. Estado do host

Vem do bridge do Lince.

Exemplos:

- `serverId`
- `viewId`
- `streams.enabled`
- estado de autenticação
- `widgetState` persistido por card

Representação recomendada:

- bridge event do host
- sinais Datastar locais

### 2. Estado de dados do backend

Vem da stream interna oficial do widget.

Exemplos:

- cards
- contagem por coluna
- campos derivados do `Record`
- mensagens de incompatibilidade de schema

Representação recomendada:

- HTML fragments por default
- signal patches quando o DOM for estável e a mudança for pequena

### 3. Estado puramente local de interação

Exemplos:

- coluna em resize
- drag em andamento
- foco no editor
- modal aberto
- texto de rascunho ainda não salvo

Representação recomendada:

- sinais Datastar quando isso não conflitar com browser APIs
- JavaScript puro quando houver drag, pointer capture, reorder visual, ou edição sensível a foco

## Runtime Recomendado

O Kanban não deve consumir diretamente apenas `/host/integrations/servers/{server_id}/views/{view_id}/stream`.

Ele deve usar uma stream interna oficial por instância do widget, por exemplo:

- `/host/widgets/{instance_id}/stream`

Essa stream interna deve:

- resolver o card pelo `instance_id`
- ler `serverId`, `viewId`, `widgetState` e estado de stream do board
- consumir a view local ou remota internamente
- validar compatibilidade com `Record`
- renderizar fragments Maud
- emitir patches Datastar

## Contrato de Dados

### Aceitação mínima

A view SSE só é compatível quando for centrada em `Record`.

O widget deve rejeitar a stream com erro curto de incompatibilidade quando:

- não houver `id`
- não houver `quantity`
- não houver `head`
- não houver `body`
- a view não parecer centrada em `record`

### Regra de compatibilidade

A view é aceita se:

- contém as colunas obrigatórias de `Record`
- os rows são compatíveis com essas colunas
- a query ou metadados indicam fonte principal em `record`

### Colunas obrigatórias

- `id`
- `quantity`
- `head`
- `body`

### Colunas opcionais suportadas

Estas colunas podem existir na view e devem ser aproveitadas quando presentes:

- `bucket`
- `bucket_label`
- `start_date`
- `end_date`
- `estimate_consumption`
- `actual_consumption`
- `assignee_id`
- `assignee_name`
- `parent_id`
- `depth`
- `children_count`
- `comments_count`
- `comments_preview`

Se não existirem, o widget deve:

- continuar funcionando com o núcleo do Kanban
- esconder a UI dependente desses campos
- não inventar valores

## Observação Importante de Schema

Hoje o schema central de `record` ainda é só:

- `id`
- `quantity`
- `head`
- `body`

Então:

- CRUD total de `head`, `body` e `quantity` é suportável já
- bucket, datas, estimativas, pessoa, hierarquia e comentários precisam ser lidos de colunas opcionais da view ou de um schema complementar
- enquanto esse schema complementar não existir, esses campos devem ser tratados como leitura opcional

Para que todas as features sejam totalmente editáveis, será necessário suporte backend explícito para metadados de task e comentários.

## Mapeamento de Colunas de Kanban

Mapeamento atual de `quantity`:

- `0` => Backlog
- `-1` => Next
- `-2` => WIP
- `-3` => Review
- `1` => Done

Esse mapeamento deve continuar por compatibilidade.

## Layout e Estrutura

## Shell fixo

O shell do widget deve ser estável e renderizado por Maud.

Regiões fixas:

- header do widget
- toolbar de conexão e visualização
- área de board
- footer opcional de status

### Regiões com patch HTML

Estas regiões devem ser alvo preferencial de HTML fragments:

- `#kanban-header-meta`
- `#kanban-columns`
- `#kanban-empty-or-error`
- `#kanban-create-sheet`
- `#kanban-edit-sheet`

### Regiões com signal patches

Estas regiões devem ser atualizadas preferencialmente por sinais:

- estado de conexão SSE
- estado pausado ou ativo
- coluna em resize
- colunas recolhidas
- modo global de body
- modo individual de body por card
- drawer/modal aberto
- rascunhos locais

## Responsabilidades por tecnologia

## Maud

Maud deve construir:

- shell base do widget
- loading state
- locked state
- mismatch state
- erro curto de incompatibilidade
- header com status e ações
- colunas e cards
- formulários de criação e edição
- estados vazios

## Datastar

Datastar deve controlar:

- toggles visuais
- `data-show`
- `data-class`
- `data-style`
- estado persistido por card
- modo de exibição do body por card
- modo global de exibição do body
- colunas recolhidas
- largura persistida das colunas
- estado de conexão renderizado no shell
- integração com bridge do host

## JavaScript puro

JavaScript puro deve ficar responsável por:

- drag and drop entre colunas
- pointer-driven resize de colunas
- scroll UX especial quando necessário
- edição com foco delicado
- atalhos de teclado que precisem de controle fino

## Persistência

Todo estado de interface do componente deve ser persistido dentro do `widgetState` do card.

Persistir:

- larguras de colunas
- colunas recolhidas
- modo global do body
- modo individual do body por card
- filtros locais
- preferência de stream pausada
- estado de sheets ou drawers quando fizer sentido

Não persistir:

- tokens
- autenticação
- dados sensíveis remotos
- conexão viva em si

## Estrutura visual do card

Cada card deve suportar:

- `Head`
- `Body`
- `Bucket`
- datas de início e fim
- estimativa e consumo real
- responsável
- indicadores de hierarquia
- comentários

### Modos do body

Cada card deve suportar:

- oculto
- trecho pequeno
- completo

Também deve existir:

- toggle global para todos os cards
- override individual por card

## CRUD de Record

O Kanban deve suportar CRUD completo do núcleo de `Record`.

### Criar

Deve permitir criar card com:

- `head`
- `body`
- `quantity`

Se campos opcionais existirem na stack backend para o projeto atual, o formulário também pode expor:

- bucket
- datas
- estimativas
- pessoa
- relação de parent

### Editar

Deve permitir editar:

- `head`
- `body`
- `quantity`

E, quando houver suporte backend:

- bucket
- datas
- estimativas
- pessoa
- parent

### Mover entre colunas

Mover card entre colunas deve:

- atualizar `quantity`
- fazer update otimista local
- confirmar por request write mediada pelo host
- reverter em caso de falha

### Deletar

Deve permitir deletar cards.

Requisitos:

- ação explícita
- feedback visual
- remoção otimista opcional
- rollback em caso de erro

## Comentários

Comentários devem existir na especificação do widget, mesmo que a persistência completa dependa de schema adicional.

Suporte mínimo:

- mostrar contagem quando `comments_count` existir
- mostrar preview quando `comments_preview` existir

Suporte completo desejado:

- abrir lista de comentários do card
- adicionar comentário
- editar comentário
- deletar comentário

## Hierarquia

Hierarquia deve existir na especificação.

Suporte mínimo:

- mostrar relação visual quando `parent_id`, `depth` ou `children_count` existirem

Suporte completo desejado:

- indentação visual
- criação de subtarefa
- navegação parent/child

## Buckets, Datas, Estimativas e Pessoas

Esses recursos devem ser tratados como campos opcionais de alto valor.

Quando as colunas existirem na view:

- renderizar
- permitir filtro local se fizer sentido
- permitir edição apenas se houver write support real

Quando não existirem:

- não renderizar placeholders falsos
- não mostrar inputs quebrados

## Conexão SSE e Auth

O widget deve deixar claro:

- se o stream está vivo
- se o stream está pausado
- se o stream está desligado
- se a autenticação caiu
- se precisa reconectar

### Ações de stream

Devem existir ações separadas para:

- pausar recebimento
- retomar recebimento
- reconectar

Se fizer sentido no runtime final:

- toggle único de stream com estado explícito

### Regras de auth

- usar o login geral da web
- nunca implementar login interno no componente
- quando auth cair, mostrar estado claro e curto
- permitir reabrir o fluxo geral de conexão via host

## Scroll e Altura

O Kanban deve ocupar a altura inteira do componente.

Requisitos de scroll:

- scroll horizontal do board inteiro quando as colunas excederem a largura
- scroll interno da página do componente
- scroll individual por coluna

Isso implica:

- shell com altura total
- área principal com overflow controlado
- colunas com listas internas roláveis

## Resize e Recolhimento de Colunas

Cada coluna deve suportar:

- resize por drag
- persistência da largura
- recolher/minimizar

O resize deve ser JavaScript puro com persistência em sinais e `widgetState`.

O recolhimento deve ser Datastar-first.

## Estados de UX obrigatórios

O widget deve ter estes estados explícitos:

- sem configuração
- aguardando primeira snapshot
- ativo com SSE viva
- pausado
- auth caída
- incompatibilidade com schema
- erro de backend
- vazio

Todos devem ser compactos e claros.

## Patch Strategy Recomendada

### HTML fragments

Usar para:

- colunas
- cards
- listas
- estados vazios
- loading/error body

### Signal patches

Usar para:

- estado de conexão
- badge de live status
- paused/running
- body mode global
- body mode individual
- colunas recolhidas
- larguras persistidas

### Regras

- não re-renderizar o root inteiro em toda snapshot
- patchar regiões internas estáveis
- preservar estado local sempre que possível

## Checklist Funcional

### Já esperado no spec final

- [x] Salvar estado do componente no `board-state.json`
- [x] Puxar dados por `View SSE`
- [x] Mapear colunas por `quantity`
- [x] Permitir scroll individual por coluna

### Obrigatório implementar

- [ ] Ler apenas dados compatíveis com `Record`
- [ ] Rejeitar outras sources com erro curto de incompatibilidade
- [ ] CRUD completo de `Record`
- [ ] Criar cards
- [ ] Editar `head`
- [ ] Editar `body`
- [ ] Alterar `quantity` via drag entre colunas
- [ ] Deletar cards
- [ ] Ocupar altura inteira do componente
- [ ] Scroll horizontal do board inteiro
- [ ] Scroll interno do componente como página
- [ ] Resize de colunas por drag
- [ ] Recolher colunas
- [ ] Body por card em oculto, trecho pequeno e completo
- [ ] Toggle global do body
- [ ] Botão para pausar stream
- [ ] Botão para retomar ou reconectar stream
- [ ] Estado visual claro de SSE viva
- [ ] Estado visual claro de auth caída
- [ ] Usar login geral da web

### Recursos avançados que entram no spec mas dependem de schema write explícito

- [ ] Bucket editável
- [ ] Datas editáveis
- [ ] Estimativa e consumo real editáveis
- [ ] Atribuição de pessoa editável
- [ ] Hierarquia editável
- [ ] Comentários com CRUD

## Recomendações práticas de implementação

### Primeira fase

Implementar primeiro:

- validação estrita de `Record`
- shell Sand Class B
- stream interna oficial por instância
- HTML fragments para colunas e cards
- Datastar signals para body mode, colapso e status
- CRUD de `head`, `body`, `quantity`
- delete
- layout de altura total e scrolls corretos

### Segunda fase

Implementar depois:

- resize de colunas por drag
- create/edit sheets mais ricos
- suporte opcional a colunas extras da view
- status de conexão mais refinado

### Terceira fase

Entrar quando o backend suportar:

- metadados ricos de task
- comentários
- hierarquia com write
- pessoas

## Decisão Final

O Kanban oficial de `Record` deve ser implementado como `Sand Class B: Hybrid Reactive`.

Regra principal:

- Maud para estrutura
- Datastar para estado local e reatividade do widget
- JavaScript puro só para drag, resize e interações finas
- HTML fragments como transporte principal da stream oficial
- signal patches como complemento

Esse é o melhor ponto de equilíbrio entre capacidade, clareza arquitetural e futura expansão.
