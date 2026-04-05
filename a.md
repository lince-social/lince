› Ok now it works. Make it so in edit mode the option to Add card has the correct text. Its not hub remoto em
main. Also i have Lince manas (lince vps) very well confired throghout my application, but i dont see it in
my dna lookup for servers, i only see my local one. Please make it so all the servers i have connected
appear there. If they have no sand show that they dont.

Remove this sand-flags.json, i didnt ask for it. I just want one sand flag, thats it.

# Web

If you are building Sand (Web's widgets/components) and are using a dependency that requires the licence to be present please include it in the html

Yeah i dont plan in changing organ then. I want versioned sand, please do it how you think its best. For sand flags: remove it. Its ok to create them every run if they have different hash from bucket/local ones, just make it so that if it fails to bucket/local create it prints/logs it and continues with the day. Im thinking of doing the official organs when seeing dna sands in search to be hardcoded to the endpoints i set because that way anyone posting normal components will not be shown in mine to be official according to the web version's code. That hardcoding will be well managed, dont worry.

I agree on the hardcoding critique, what do you recommend? The thing with maintaining official is that how can i control an official if it is peer to peer? What do you recommend on the sand publication aware? On same version different hash make a harsh policy. You are right, dont seed manas organ.

Actually go back to official/community. I like it. Tell me more about officially signature based part. What parts it envolved, what is the artifact/key, where is it saved, how its generated, shared and used. How does the interaction to use it look like, what data it changes (i want to know more bc i dont want to have to change it in the long term, already have a good thing now). For sand: dont try to mess with remote organs publish, care only about your own, the rest about bucket i agree with, explain the official sand sync instead of checking for hashes. I think actually we should have a way for me to block the generation of all sand every startup with either bucket/local creation. And be opt in on all or some sand to be created from the ones inside lince. I think we need to put that in a web/sand.toml or something. I like the idea of signature based to not bless bad sand. i want to figure out some retention pruning default policy.

| Thing                                      | Meaning                                                                               | Persists where                                | Mutates truth? |
| ------------------------------------------ | ------------------------------------------------------------------------------------- | --------------------------------------------- | -------------- |
| origin                                     | Which organ + SSE view is feeding the widget                                          | host/card config, not widget-local ergonomics |
| Yes, because it changes the source binding |
| configuration                              | Physics and layout ergonomics like charge, link distance, collision, pinning behavior |

widgetState if it is per instance; record_extension.freestyle_data_structure if you want it attached to the
record/package instead | No, it only changes presentation |
| filters | Which categories/nodes/edges are visible or selectable | widgetState | No, it only changes the
projection |
| category membership | The actual category assigned to a record | DB-side data, not filter state | Yes |
| remove / disconnect | Remove the parent edge or clear a relation | host action against the relation table |
Yes |

When i said remove i meant the remove widget from screen, dont have a button for that (if i disconnect them by hand it happens). I understand your distinction on different filters, one is persistent other is not right? Searching by name would not be and by category would?

I think this whole filter search configuration is getting too complicated by you. Its simple: for now make category editing be persistent, meaning it changes the view just like kanban, altering the final created sse view. essentially filtering in sql. There should be a way to configure that in editing mode, thats it. Dont make search feature yet. Filter by parent too (if inputted text matches any part of head). Connections and disconnections should be an editing of relations that are sent to the db. Have a way to set the physics of the widget locally in widget state.

Please document the signature part. I think i like the general idea of verification with keys but i dont see a point right now, please make it only official/community for now (dont hardcode the official hosts). For sand i think its better to actuall yes have the controll of the official widgets that get created when, but i dont think anymore that the official should go on the bucket automatically, the user should put them there by sand upload if they want to, that way we dont have repetitive official widgets being pushed every time (they stay for local access of each user if they want to). Make the process of sand verification automatic, no user envolvement. The creation is decided by them, if they dont configure (for now) all the official are upserted at startup. I like your pruning policy.

Please help me make something the Lince way that upserts bucket widgets with the latest sand/ widgets. Dont do pinned for now. What you recommend for the version byte release policy?

Help me think of a way to not need the configuration by file for a non technical user
Then make separation inside sand/ that is for those that get sent to bucket publicly and others.

I had an insight, we have a way of doing this bucket sync, its a command.

For now lets simplify. Make the bucket updating of a sand manual with the sand export widget not automatic in any way. At startup create all the built in locally.
