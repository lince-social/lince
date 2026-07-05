The AI model looks at the user's data that the user marks as 'Allowed For AI'. The Karma, Records, Transfers, etc are analysed to sugest, in the Ask mode, to automatically change, in the Agent mode, or to simply do the optimizations it think you'll need, in Tinkerer mode.

        - [ ] Agent driven change by the user's request. When they ask the AI to change some data it performs the task.
        Bringing a short feedback about the success/failure, allowing the user to try again or look at the data and change it by hand.

        The workflows of automatic recommendation (with full access for instant change or asking for permission) must be:
        - [ ] Karma: for habits or purchases.
        - [ ] Records.

        fazer otimizações balanceamento de atividades ao longo da semana pra nao sobrecarregar um dia. Sugerir habitos novos...

# Rebirth: this AI is Fiote (docs/fable-improvement.md)

The agent is Fiote, the Lince cub. Ask/Agent/Tinkerer map onto one delegated autonomy ladder the user sets per scope:

- **observe**: read only what is marked 'Allowed For AI' (reads through Protein, so visibility rules apply automatically).
- **suggest** (Ask mode): put drafts into the Decision Queue for the user to approve.
- **draft** (Agent mode): on the user's request, create rules/records/proposals that await one-tap approval, with short success/failure feedback and retry.
- **act-within-budget** (Tinkerer mode): apply optimizations directly within configured limits.

Everything Fiote does is ordinary data — records, Karma rules, promises — written through Actions with `cause = fiote` on every fact: inspectable, reversible, and reviewable like any human change. Nothing Fiote does is a different kind of thing; it turns the same knobs the user could turn themselves (creating the same Karma automations, approving the same proposals a rule could approve). Without Fiote everything still works LLM-less; with it, whisper quality rises and delegated decisions leave the queue on their own. The week-balancing and habit-suggestion ideas above are Tinkerer-mode rule drafts over Ledger history.