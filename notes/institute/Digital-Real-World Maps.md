Being able to see the world or a digital space with it's actors and needs/contributions.
      - [ ] One can see the world as a plane with lines for the streets.
        - [ ] Bonus points for terrain data, elevation, like mountains. With that in rendering we can portrait a more accurate picture of the world and also use the elevation to show the Needs and Contributions in a 3d way. If there are a lot of Needs in one area that is like a mountain visually.
        - [ ] Integrate that with Transfer Proposal. Being able to accompany the whole process through the maps, like a delivery; understanding who is closest to Contribute to your Need.

        https://github.com/orgs/Far-Beyond-Pulsar/discussions/40

        Maybe the way to go is using a game engine in gpui like Pulsar if it allows for the rendering of a Component in a canvas or something similar to display like a game level.

# Rebirth (docs/fable-improvement.md)

Place is Lince's first **Instinct**: a built-in concept with engine functions — world coordinates or address, `distance(a, b)`, `route(a, b)` with A* over map data (path, ETA, alternatives), `near(place, radius)`, `within(place, area)` — callable from Karma conditions and Protein queries. This map is the *rendering* of what the engine already computes:

- Records and open promises carry `place`; the terrain overlay (need-mountains, surplus-valleys) is a Protein aggregation over them, gated by visibility.
- "Who is closest to Contribute to your Need" is Senses matching on route/window overlap, not a map-side calculation.
- Accompanying a delivery through the map is watching a promise: its window, its route, its state changes — the Transfer proposal integration falls out of promises having place + window.
- Map data (e.g. OSM extracts, elevation) loads as a resource; live data like traffic would arrive as Signals.