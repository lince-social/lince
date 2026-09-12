# Everyday work with other people

The earlier “Part A” notes described using Lince in a private company. The useful requirement is general: people connect to an Organ and manage knowledge, teams and tasks through native Sands.

A person can sign in, create and find Records, configure Protein views, use Table, Todo or Kanban, assign work and discuss a Record. An administrator can create Actors and Roles, choose readable Records and editable properties, preview access, recover credentials and revoke access from open clients. These pieces should work from an empty Organ with manual setup.

Use the same Record and Assertion model throughout. A project can organize work without becoming a mandatory owner of every task. A displayed Protein filter is a view; backend permission checks decide what the person can read or change, including the result of a proposed edit. A permitted change may remove a card from its current view.

Those checks include changes to Concepts, relationships and membership that would change access indirectly. A copied `admin` label is not the protected Concept's identity. Signing in as a Person is also different from enrolling a privileged Organ Cell.

One shared editor preserves collaborative text, private drafts, locked descriptions and useful refusals. Activity, Trash, restore and protected backup information complete the everyday journey. A second person connecting from another network should be able to use it, and access loss, restart and recovery should remain understandable.

Distinguish a draft acknowledged by the server from an outage edit held only in memory. Access loss and identity changes follow the data's retention rules. Locked descriptions save ciphertext only and clear unlocked content when its authorized session ends. Undo or restore rechecks current access and cannot retract another person's later work or restore an old permission grant. Administrators need a recovery route when the server is down; ordinary users must not receive its private reports or credentials.

The older task codes, builder assignments and dependency graph have been removed from this note. The proposed wording now lives in [Everyday use](interface.md#everyday-use), [Records and collaboration](interface.md#records-and-collaboration) and [Checking the result](interface.md#checking-the-result). Take implementation order from the owner's [Lince.lingua](../../Lince.lingua).
