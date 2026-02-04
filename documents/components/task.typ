// Status enum values: "wip", "todo", "done"
// Priority: wip > todo > done (for grouping purposes)

// Check if shadows are enabled via input variable (default: false)
#let use-shadows = sys.inputs.at("shadows", default: "false") == "true"

// Check if dark mode is enabled via input variable (default: false)
#let dark-mode = sys.inputs.at("dark", default: "false") == "true"

// Colors based on dark/light mode
#let card-bg = if dark-mode { rgb("#2a2a3e") } else { white }
#let card-stroke = if dark-mode { rgb("#4a4a5e") } else { gray }
#let line-stroke = if dark-mode { luma(50) } else { luma(70) }

#import "@preview/shadowed:0.3.0": shadow as real-shadow

// Conditional shadow function - either applies shadow or just returns content
#let maybe-shadow(blur: 0pt, spread: 0pt, fill: none, radius: 0pt, content) = {
  if use-shadows {
    real-shadow(blur: blur, spread: spread, fill: fill, radius: radius, content)
  } else {
    content
  }
}

#let status-priority = (
  "wip": 0,
  "todo": 1,
  "done": 2,
)

#let status-colors = (
  "wip": rgb("#3b82f6"),
  "todo": rgb("#9ca3af"),
  "done": rgb("#22c55e"),
)

#let status-shadow-fills = (
  "wip": gradient.linear(
    rgb("#3b82f6").lighten(40%),
    white,
    rgb("#87ceeb"),
    white,
    rgb("#3b82f6").lighten(60%),
  ),
  "todo": gradient.linear(
    rgb("#9ca3af").lighten(30%),
    rgb("#d1d5db"),
    rgb("#9ca3af").lighten(40%),
  ),
  "done": gradient.linear(
    rgb("#22c55e").lighten(40%),
    rgb("#86efac"),
    rgb("#22c55e").lighten(50%),
  ),
)

#let status-labels = (
  "wip": "Work In Progress",
  "todo": "TODO",
  "done": "Done",
)

// Get the highest priority status from a list of contributors
// Each contributor is a tuple: ("name", "status")
#let get-task-status(contributors) = {
  let best-status = "done"
  let best-priority = status-priority.at("done")

  for contributor in contributors {
    let (name, status) = contributor
    let priority = status-priority.at(status, default: 2)
    if priority < best-priority {
      best-priority = priority
      best-status = status
    }
  }

  best-status
}

// Format contributors with their status badges
#let format-contributors(contributors) = {
  contributors
    .map(contributor => {
      let (name, status) = contributor
      let color = status-colors.at(status, default: gray)
      [#name #box(fill: color.lighten(70%), outset: 2pt, radius: 2pt, text(
          size: 0.8em,
          fill: color.darken(20%),
          weight: "bold",
          upper(status),
        ))]
    })
    .join([, ])
}

// Create a task data structure
#let task-data(
  title,
  contributors: (),
  type: "Other",
  body,
) = {
  (
    title: title,
    contributors: contributors,
    type: type,
    body: body,
    status: get-task-status(contributors),
  )
}

// Render a single task card
#let render-task(task) = {
  let status-color = status-colors.at(task.status, default: gray)

  block(
    width: 100%,
    inset: 1em,
    stroke: (left: 3pt + status-color, rest: 0.5pt + gray),
    radius: 4pt,
  )[
    *#task.title* \

    _Contributors: #format-contributors(task.contributors)_

    #line(length: 100%, stroke: 0.3pt + luma(70))

    #task.body
  ]
}

// Render a status group header
#let render-status-header(status, task-count) = {
  let color = status-colors.at(status, default: gray)
  let shadow-fill = status-shadow-fills.at(
    status,
    default: gray.transparentize(50%),
  )
  let label = status-labels.at(status, default: status)

  maybe-shadow(blur: 10pt, spread: 1pt, fill: shadow-fill, radius: 8pt)[
    #block(
      width: 100%,
      fill: color,
      inset: (x: 1em, y: 0.5em),
      radius: 8pt,
    )[
      #text(fill: white, weight: "bold", size: 1.2em, label)
      #h(1fr)
      #text(fill: white.darken(10%), size: 0.9em, [(#task-count tasks)])
    ]
  ]
}

// Render a single task with shadow
#let render-task-with-shadow(task) = {
  let status-color = status-colors.at(task.status, default: gray)
  let shadow-fill = status-shadow-fills.at(
    task.status,
    default: gray.transparentize(50%),
  )

  maybe-shadow(blur: 30pt, spread: 1pt, fill: shadow-fill, radius: 4pt)[
    #block(
      width: 100%,
      inset: 1em,
      stroke: (left: 3pt + status-color, rest: 0.5pt + card-stroke),
      radius: 4pt,
      fill: card-bg,
    )[
      *#task.title* \

      _Contributors: #format-contributors(task.contributors)_

      #line(length: 100%, stroke: 0.3pt + line-stroke)

      #task.body
    ]
  ]
}

// Render a type subgroup header
#let render-type-header(type-name, task-count, status) = {
  let color = status-colors.at(status, default: gray)

  block(
    width: 100%,
    fill: color.lighten(70%),
    inset: (x: 0.8em, y: 0.4em),
    radius: 4pt,
    stroke: 0.5pt + color.lighten(40%),
  )[
    #text(fill: color.darken(20%), weight: "bold", size: 1em, type-name)
    #h(1fr)
    #text(fill: color.darken(10%), size: 0.8em, [(#task-count)])
  ]
}

// Get unique types from tasks
#let get-unique-types(tasks) = {
  let types = ()
  for task in tasks {
    if task.type not in types {
      types.push(task.type)
    }
  }
  types
}

// Render a status group box
#let render-status-group(status, tasks) = {
  if tasks.len() == 0 { return }

  render-status-header(status, tasks.len())
  v(0.5em)

  let types = get-unique-types(tasks)

  for type-name in types {
    let type-tasks = tasks.filter(t => t.type == type-name)
    if type-tasks.len() > 0 {
      render-type-header(type-name, type-tasks.len(), status)
      v(0.3em)

      for task in type-tasks {
        render-task-with-shadow(task)
        v(0.5em)
      }
    }
  }
}

// Main task board function - renders all tasks grouped by status
#let task-board(tasks) = {
  let wip-tasks = tasks.filter(t => t.status == "wip")
  let todo-tasks = tasks.filter(t => t.status == "todo")
  let done-tasks = tasks.filter(t => t.status == "done")

  render-status-group("wip", wip-tasks)

  if wip-tasks.len() > 0 and todo-tasks.len() > 0 { v(1em) }

  render-status-group("todo", todo-tasks)

  if (wip-tasks.len() > 0 or todo-tasks.len() > 0) and done-tasks.len() > 0 {
    v(1em)
  }

  render-status-group("done", done-tasks)
}

// Legacy task function for backwards compatibility (defaults to todo status)
#let task(
  title,
  contributors: (),
  body,
) = {
  // Convert old format ("name",) to new format (("name", "todo"),)
  let new-contributors = contributors.map(c => {
    if type(c) == array {
      c
    } else {
      (c, "todo")
    }
  })

  render-task(task-data(title, contributors: new-contributors, body))
}
