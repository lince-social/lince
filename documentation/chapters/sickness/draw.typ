The strongest pure-JS path for Lince is:

- Camera: infinite-viewer or \@panzoom/panzoom
- Drawing scene: svg.js
- Freehand strokes: perfect-freehand
- Sketch look: roughjs if you want Excalidraw-like rough shapes
- Widget transforms: moveable
- Box selection: selecto

All of those are MIT-licensed in their official docs / repos as of April 6, 2026:

- svg.js: MIT, SVG manipulation library: https://github.com/svgdotjs/svg.js
- svg.panzoom.js: SVG pan/zoom plugin: https://github.com/svgdotjs/svg.panzoom.js
- perfect-freehand: MIT, pressure-sensitive stroke generation: https://github.com/steveruizok/perfect-freehand
- roughjs: hand-drawn rendering for Canvas or SVG: https://github.com/rough-stuff/rough
- moveable: MIT, draggable/resizable/rotatable/snappable: https://github.com/daybrush/moveable
- selecto: MIT, drag-area multi-select: https://github.com/daybrush/selecto
- infinite-viewer: MIT, infinite document viewport: https://github.com/daybrush/infinite-viewer

My recommendation is to prefer an SVG-based whiteboard over a canvas-first one.

Why:

- Your widgets are real DOM/iframe objects today, not bitmap shapes.
- SVG drawings plus DOM widgets on sibling layers are much easier to keep in one camera space.
- Canvas engines like Konva and Fabric are good, but they are canvas scenegraphs. Live widgets would still need a separate DOM overlay anyway.

That means the workspace should look like this:

// <div class="workspace-viewport">
//   <div class="camera-root" style="transform: matrix(...)">
//     <svg class="scene-layer"></svg>
//     <div class="widget-layer"></div>
//     <div class="interaction-layer"></div>
//   </div>
// </div>

And your saved workspace becomes:

{
"id": "space-1",
"name": "Area 1",
"camera": { "x": 0, "y": 0, "zoom": 1 },
"drawings": [
{ "id": "d1", "tool": "pen", "points": [[0,0,0.5],[10,8,0.6]], "style": {...} },
{ "id": "d2", "tool": "rectangle", "x": 100, "y": 200, "w": 300, "h": 180, "style": {...} }
],
"widgets": [
{ "id": "w1", "x": 1200, "y": 900, "w": 640, "h": 420, "packageName": "kanban_record_view", "widgetState": {} }
]
}

If you want the closest thing to Excalidraw features without using Excalidraw, I would build this feature set in phases:

1. Infinite camera
  Use infinite-viewer or panzoom to get pan/zoom and save camera per workspace.
2. Whiteboard drawing
  Use perfect-freehand for pen strokes and store points as vectors, not pixels.
3. Shape tools
  Use plain SVG elements or svg.js objects for rectangles, ellipses, arrows, text, and notes.
4. Rough style
  Use roughjs only for shapes that should look hand-drawn. Keep freehand ink separate.
5. Widget objects
  Keep widgets as absolutely positioned DOM/iframe nodes inside the same camera root.
6. Selection / resize / snap
  Use moveable and selecto for widgets first. Later, add similar controls for SVG shapes.

There are two other viable directions, but both have tradeoffs:

- Konva
  Official docs describe it as an HTML5 Canvas 2D framework with layers, events, drag/drop, transformer handles, and JSON serialization. It is MIT. It is very good
  for whiteboard drawing and editor interactions.
  Source: https://konvajs.org/docs/ and https://konvajs.org/docs/guides/why-konva.html
  Tradeoff:
  Since Konva is canvas-based, live Lince widgets would have to remain in a synchronized DOM overlay. That is workable, but mixing canvas shapes with DOM widgets is
  more awkward than SVG + DOM.
- Fabric.js
  MIT, strong object model, built-in free drawing (isDrawingMode), transforms, grouping, JSON/SVG IO.
  Sources: https://github.com/fabricjs/fabric.js and https://fabricjs.com/docs/old-docs/fabric-intro-part-4/
  Tradeoff:
  Same issue as Konva, but I think it is a worse fit for your widget-heavy workspace.

So the practical answer is:

- If drawings are the priority and widgets are secondary: Konva.
- If widgets and drawings must feel equally native on the same plane: SVG.js + perfect-freehand + moveable/selecto + infinite-viewer.

For Lince, I would choose the second one.

It matches your current architecture better:

- pure JS
- vendorable minified libs with attached licenses
- Datastar can stay in the shell
- widgets remain real iframes
- workspaces remain separate scene documents you can switch between

If you want, I can turn this into a concrete Lince architecture proposal for crates/web/static/presentation/board/, including file/module boundaries and the exact
scene data model.
