#let kanban-item(
  date,
  inset: 0.27em,
  stroke: 0.05em,
  fill: white,
  height: auto,
  ..args,
) = {
  let assignee
  let name
  if args.pos().len() == 1 {
    name = args.pos().first()
  } else {
    (assignee, name) = args.pos()
  }
  let rect(fill: gray.darken(50%), color: white, body) = std.rect(
    fill: fill,
    inset: inset,
    radius: 0.2em,
    text(color, body),
  )
  assignee = if assignee != none { rect.with(assignee) } else { (..a) => none }
  let stroke = if type(stroke) == color { std.stroke(stroke + 0.05em) } else {
    std.stroke(stroke)
  }
  let left-stroke = std.stroke(
    paint: stroke.paint,
    thickness: stroke.thickness + 0.5em,
    cap: stroke.cap,
    join: stroke.join,
    dash: stroke.dash,
    miter-limit: stroke.miter-limit,
  )
  grid.cell(
    box(
      stroke: (left: left-stroke, rest: stroke),
      inset: (left: stroke.thickness / 2),
      radius: 0.3em,
      box(
        width: 100%,
        height: height,
        fill: fill,
        stroke: (rest: stroke),
        inset: 0.5em,
        radius: 0.3em,
        align(
          horizon,
          stack(
            spacing: 0.5em,
            name,
            stack(
              dir: ltr,
              spacing: 0.5em,
              rect(fill: green.lighten(10%))[#date],
              assignee(fill: blue),
            ),
          ),
        ),
      ),
    ),
  )
}

#let kanban-column(name, color: auto, ..items) = {
  (name: name, color: color, items: items.pos())
}

#let kanban(
  width: 100%,
  item-spacing: 0.5em,
  leading: 0.5em,
  font-size: 1em,
  ..args,
) = {
  let columns = args.pos()
  let column-names = columns
    .enumerate()
    .map(((i, x)) => table.cell(
      stroke: (bottom: stroke(paint: columns.at(i).color, thickness: 1pt)),
      align: left,
      inset: (bottom: 0.5em, rest: 0pt),
      [#x.name (#columns.at(i).items.len())],
    ))
  let column-items = columns.map(x => x.items)
  set text(size: font-size)
  set par(leading: leading)
  show table: set par(justify: false)
  block(
    width: width,
    table(
      columns: (1fr,) * columns.len(),
      align: left + top,
      stroke: none,
      column-gutter: 1.5em,
      inset: 0pt,
      row-gutter: item-spacing,
      table.header(..column-names),
      ..column-items
        .map(items => grid(row-gutter: item-spacing, ..items))
        .flatten(),
    ),
  )
}
