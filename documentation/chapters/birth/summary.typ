== Summary

Lince is a tool for registry, interconnection and automation of Needs and Contributions with open scope.

The phrase above is the densest way to explain Lince. The current page serves as an overview, and the subsequent chapters cover it in much more detail. Let's start with the registry part:

*Registry*

Users will start their Lince application for the first time, and that will create a personal database

(for UNIX systems [Linux and macOS] it is located at '~/.config/lince/lince.db').

That database is a Sqlite file, that is reffered by the application as a DNA. That file can be altered to make the app behave differently.

The behavior can be of: setting personal/professional tasks, modeling personal items, scheduling computer actions, act out economic trades between parties, and many more, the sky, your imagination and computing power is the limit.

Any app behavior mentioned above can be boiled down to a Need, or a Contribution to a Need. The differentiator of the two is a Quantity. If such quantity is negative, that will be a Need.

Having -2 Apples means you need to go and get two apples to be neutral, to satiate your needs. The same applies to a habit, having a -1 Quantity in Exercise means you need to exercise.

The previous description is the basis of the whole app, everything revolves around it.

*Automation*

If there is predictability in your Needs, and a fixed frequency is enough to describe them, then a simple automation can be constructed.

One can set the automatic every day change of the 'Studying' habit's Quantity to -1. And with another feature, if that Quantity is -1 a Command is sent to the computer that closes all programs that are gaming related.

Another point of automation is the forementioned DNA. One aspect of constructing systems is that many great ideas often are lost with time, memory and the passing away of those contributors. Common patterns and knowledge about production and maintenance of commodities, economic trades, access to resources and ways of living is scattered.

These patterns could be saved into some form of database, that when fed into a program like Lince, will create Needs and Contributions, so users can have a headstart in any endeavor. From production according to a bill-of-materials to morning routines and parties organization, these activities create Needs for gathering resources and acting out tasks.

Contributions to Lince DNAs are like creating blueprints for others to get up and running. They might not be a perfect fit, but allow for great customization. Currently the way of centralizing DNAs is the [Github Repository](https://github.com/lince-social/dna).

Interconnection

With great customizability and automation of people's DNAs, the next barrier is the connection of those individual apps, turning each machine into a p2p node, also called a Lince Cell. The same way you create triggers and rules for the quantites of your DNA, you can do the same for others.

If one Market has 200 apples and you need 5 you can create a Transfer Proposal. The Quantity of money diminishes on your side, and the Apple one increases. On the Market side, a manual price can be inputed and accepted when proposals come, or it can be adaptive. Donations of clothes and asking people close to you for a tool would be similar, just without the money part.
