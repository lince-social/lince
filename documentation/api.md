-- Graph-easy + slides (installed with lspeace.nix)

# API - Views:

## /api/view/queries:

```
~~~graph-easy --as=boxart
[ \[ SELECT * FROM record, SELE..., ... \] ] - to -> [ Main.tsx ] -> [Failure]
~~~
```

## /api/view/names:

```
~~~graph-easy --as=boxart
[ \[ Init View, Second View, ...\] ] - to -> [ Bar.tsx ] -> [Success]
~~~
```

## /api/data(viewQuery):

```
~~~graph-easy --as=boxart
[ \[ {"head": "Apple"}, ...\] ] - to -> [ Grid.tsx ] -> [map(record => <Table data={record}/>]
~~~
```
