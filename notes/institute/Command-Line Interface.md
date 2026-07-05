There is a simpler workflow for lince, the simplest of them all: the CLI. We dont need to deal with choices made for user interfaces, we care about the data being altered, and care about being able to call Lince from other programs to keep using our personal workflows.

So we might not care about Views, or Configuration tables. It might be enough to have Record to model our Needs, and Karma to automate the management of that information. If we are dealing with Karma we probably want to CRUD the tables that can be used by Karma (already counting with karma_condition and karma_consequence), so CRUD of (Shell) Commands, (SQL) Queries and Frequency (very important).
 
Here are suggestions for some of the basic arguments of Karma, no need to follow it, just a suggestion:

lince + <arguments>:
- karma ls
- karma ls <id>
- karma new <condition> <operator> <consequence>
- karma rm <id>
- karma deliver (evals all karma)
- karma deliver <karma_id or record_id>
- karma edit <id> <condition/consequence/operator> <new_value>

To deal with the arguments it is possible to do it by hand if it's not a hassle, or use the 'clap' crate, for the cool terminal visuals maybe 'crossterm' is the best candidate.

There are some cli args already, they are used for configuring if the app will run with karma or not, or with the current interface (controlled by the feature flags: gui, tui, http). If you feel like they are getting in the way of the task or you would like to change it feel free to do it.