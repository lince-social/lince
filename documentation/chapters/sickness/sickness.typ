#import "../components/major.typ": major

#major(
  "Sickness",
  $"Cough"$,
  message: "`Morbus est medicus naturae` \\Disease is the doctor of nature, illness forces correction",
  by: "ChatGPT (circa 2025)",
)

Lince is not great, it always needs to get better, until it dies, then it doesn't.
The way to do it is by giving it medicine (teaching people how to use it) while we cure it (making it simpler and featureful).
The tasks and community to improve are found in the Discord server. So for now, with this configuration, we can focus on
improving your usage of Lince.

Making a Lince DNA, modeling items and actions to perform is a continuous effort, if it makes sense to you.

== Good practices
-- Create a view or more with all the data and configuration for managing your DNA, so Configurations, Collections, Views...

*Karma*

Until Lince has Deterministic Simulation Testing (DST), you have to be mindfull of the Command table you produce, every command you set may possibly break your system if you don't tidy things up. If you have logic of running a command every hour if one record has quantity > X and you forget about it, any simple change will trigger it, so put guardrails for running things, be it Commands, Queries or even changing Record Quantities. Changes might cascade and deliver unforeseen consequences.

With DST this is easier to do, the plan is to have a containerized environment that runs a simulation of your system, isolated from the outside world. Your DNA (your personal configuration of lince) is a seed that can be run multiple times arriving at the same result (hopefully). Being able to run it with a high speed, changing the date (affecting the Frequency table) and record quantities will bring reproducible results. When you want to add something to your DNA you can check it's effects with a simulation and get info if it breaks anything in an edge case.

*Command*

Lince works with Sqlite files for it's database. It is recomended to frequently backup your DNAs, weekly, daily or hourly if you are paranoid. If some error or mistake happens, your information is safe.

=== Personal Tasks
-- Create Views for understanding your tasks, select the ones that have negative quantity, check this SQL:
```sql
SELECT * FROM record WHERE quantity < 0 AND LOWER(head) LIKE '%task%'
```
-- One can use the 'head' column in 'record' table as a tag holder.
So Record's heads with Items and Tasks that have negative quantities appear in my 'Negatives' view.

=== Finance

=== Transactions
