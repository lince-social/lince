# Remaining canvas topology work

- [ ] Support caves, overhangs, and multiple surfaces at one canvas position. Replace height-only queries with surface intersections, normals, and support queries while retaining independent effects and generic bindings.
- [ ] Separate an Area's drawn shape, affected region, and force direction. Provide surface patches, planes with explicit thickness, extruded regions, and spheres, with one shared region query for all Area capabilities.
- [ ] Support attraction toward a chosen 3D target within these regions, including Sands inside caves. Collision determines whether a Sand can leave the cave.

Optional extension: add visibility or obstruction rules separately from region membership.
