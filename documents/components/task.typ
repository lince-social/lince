// Status enum values: "wip", "todo", "done"
// Priority: wip > todo > done (for grouping purposes)

// Check if dark mode is enabled via input variable (default: false)
#let dark-mode = sys.inputs.at("dark", default: "false") == "true"

// Colors based on dark/light mode
#let card-bg = if dark-mode { rgb("#2a2a3e") } else { white }
#let card-stroke = if dark-mode { rgb("#4a4a5e") } else { gray }
#let line-stroke = if dark-mode { luma(50) } else { luma(70) }

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

#let status-labels = (
  "wip": "Work In Progress",
  "todo": "TODO",
  "done": "Done",
)

// Get the highest priority status from a list of contributors
// Each contributor is a tuple: ("name", "status")
#let get-task-status(contributors) = {
  let best-status = none
  let best-priority = 999

  for contributor in contributors {
    let (name, status) = contributor
    let priority = status-priority.at(status, default: none)
    if priority != none and priority < best-priority {
      best-priority = priority
      best-status = status
    }
  }

  if best-status == none { "todo" } else { best-status }
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

#let task(
  title,
  contributors: (),
  body,
) = {
  (
    title: title,
    contributors: contributors,
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
    stroke: (left: 3pt + status-color, rest: 0.5pt + card-stroke),
    radius: 4pt,
    fill: card-bg,
  )[
    *#task.title* \

    _Contributors: #format-contributors(task.contributors)_

    #line(length: 100%, stroke: 0.3pt + line-stroke)

    #task.body
  ]
}

// Render a status group header
#let render-status-header(status, task-count) = {
  let color = status-colors.at(status, default: gray)
  let label = status-labels.at(status, default: status)

  block(
    width: 100%,
    fill: color,
    inset: (x: 1em, y: 0.5em),
    radius: 8pt,
  )[
    #text(fill: white, weight: "bold", size: 1.2em, label)
    #h(1fr)
    #text(fill: white.darken(10%), size: 0.9em, [(#task-count tasks)])
  ]
}

// Render a status group box
#let render-status-group(status, tasks) = {
  if tasks.len() == 0 { return }

  render-status-header(status, tasks.len())
  v(0.5em)

  for task in tasks {
    render-task(task)
    v(0.5em)
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
