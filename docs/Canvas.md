We need a place to put components, it needs to be very big (dozens and dozens of them, a lot lot. We probably want to do more than this, and make the frontend experienced carried by this canvas feature, so something more homebrew and less dependencies might give us freedom to rewrite how we want instead of changing parameters and being limited down the line. Just a direction I would take - Duds.

Features:
- [ ] An infinite 2D canvas
- [ ] Be able to draw (arrows, boxes, text and erasing at first is ok).
- [ ] Be able to move/resize components with drag and drop (probably in edit mode)
- [ ] Be able to add Sand to the canvas.
- [ ] Extra: would be useful to have a way to send components to z-index up and down, making some components/drawings be on top of the other at will (kinky). That way we can create new components grouped together (moved together) And dont have to reimplement the sidepanel for record creation in every Sand that could use it.