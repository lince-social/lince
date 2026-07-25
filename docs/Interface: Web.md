Hello
The base of the app should be very minimalist, to give the user the ability to be maximalist in their component extensions if they want to.

That means making every starting UI that shows the Lince logo, the AI component builder logo, and workspaces be removable. Some parts still need to exist somewhere, like the button to add new Sand (components/extensions/widgets like Kanban, Table...), that is the way to add all other components. If that is the minimum we shrink as much as possible the amount of screen we take from the user (nice).

This is the base UI necessary for all the workflow that leads to the person adding the component they will use in their day-to-day (assuming their Lince is empty):

- [ ] Background with a blank color or pattern (dots like a bullet-journal are so cool, but what do i know - duds).
- [ ] Some component to enter an edit mode, can be moved/resized, but never deleted. The way of seeing the button/s responsible for adding new components might be to enter edit mode.
- [ ] The button/s to add components should give me options to 1. Add a random component from somewhere on my pc. 2. Add a local component that Lince created and it knows where they are, displaying them like a Local Shop. 3. Opening the Network Shop, that shows the components from any of the Organs I am connected to joined together.
- [ ] Shop: a collection of components from some source (local or from my organs). There should probably be a search by Organ name and search by component name. Filter by organ too is cool? When I look at the components available i see in each card/line the Name, Organ Name (for now just these two).
- [ ] After selecting a component to add, there should be like a modal that shows the name of component, of the organ and in the future a big area for inspecting the contents of it, like the code and files (no need to do it for now). To add the component we need to click a confirm of some sorts.

Part 2: TODO later

- [ ] Entering editing mode makes the components Configurable. Being able to edit the Organ and View they point to. Also being able to click a button to swap themselves with a fresh version. Taking the same source code but updated, like a git pull (those who know..). A way to do the organ selection might be a dropdown and when selecting an organ that needs login and you are not logged in you are presented with username and password inputs and a login button maybe (all inside the component that is responsible for the organ selection). And after logging in you select the View (if the component needs one) and you are good to go.

# Design System

- [ ] Variáveis de Cor
- [ ] Biblioteca de Ícones
- [ ] Vibe:
  - [ ] Se minimalista, pelo menos engraçado, amigável. Se não tiver look minimalista, pelo menos na quantidade de informações jogamos pra baixo.
  - [ ] Tom de voz:
- [ ] Kanban, Tabela

- [ ] Biblioteca de Charts


We need a place to put components, it needs to be very big (dozens and dozens of them, a lot lot. We probably want to do more than this, and make the frontend experienced carried by this canvas feature, so something more homebrew and less dependencies might give us freedom to rewrite how we want instead of changing parameters and being limited down the line. Just a direction I would take - Duds.

Features:
- [x] An infinite 2D canvas
- [ ] Be able to draw (arrows, boxes, text and erasing at first is ok).
- [x] Be able to move/resize components with drag and drop (probably in edit mode)
- [x] Be able to add Sand to the canvas.
- [x] Extra: would be useful to have a way to send components to z-index up and down, making some components/drawings be on top of the other at will (kinky). That way we can create new components grouped together (moved together) And dont have to reimplement the sidepanel for record creation in every Sand that could use it.
