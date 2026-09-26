# Room from video

Research and recommendation: September 23, 2026. This describes a proposed workflow; it has not yet been tested on the room video.

The goal is a lightweight room made from flat surfaces and simple boxes covered with photographs. A table may be one solid box. Small details belong in the textures. Gaussian splatting remains available in Lince, but is optional for this workflow.

Prepare the visual detail before import. Lince receives a finished `.glb`; no reconstruction, LLM, or changing geometry detail is required while using the canvas. Collision can come from separate box Sands, visible for editing and hidden during normal use.

**Recommended approach: camera reconstruction, simple Blender geometry, then photo texturing.**

Use COLMAP to establish the room's spatial reference. A vision-capable agent can then help build planes and boxes in Blender, using that reference and rendered previews. OpenMVS projects the photographs onto the final shapes. The user reviews the room and decides what to simplify.

An LLM can draft a room from video alone, but its estimated sizes and positions need checking. Reconstruction tools provide measured image correspondences and camera estimates. The agent can automate Blender through Python; connecting these tools into a reliable review loop is work we would still need to implement.

| Route | Best use | Tradeoff |
| --- | --- | --- |
| COLMAP → simple Blender model → OpenMVS texturing | Preferred for deliberately crude walls and furniture boxes | Needs shape placement and review; sparse points may not describe every surface |
| COLMAP → OpenMVS reconstruction → simplify or replace geometry → texture | Best when an automatic surface is useful as a reference | More processing and cleanup; automatic simplification does not understand which objects should become boxes |
| Selected photographs → fSpy camera matching → manual Blender model and projection | Small scenes, a few key views, or reconstruction failures | More manual alignment and texture work; fSpy matches individual photographs rather than reconstructing a complete video |

**The recommended workflow, step by step:**

1. Film one room first. Move slowly, keep the same lens and zoom, and capture overlapping views under steady lighting. Include corners, furniture edges, floor, and ceiling where they matter. Keep the original video and measure one known distance, such as a wall width, to establish scale.
2. Extract sharp frames with FFmpeg or a frame-selection script. Reject blur and excessive duplicates. There is no need to process every video frame; preserve enough overlap for alignment.
3. Run COLMAP camera alignment. Keep its complete reconstruction folder and the images. Check that the cameras and room points form a consistent reconstruction before modeling. A failed alignment should be corrected before moving on.
4. Import the reconstruction into Blender with Blender Photogrammetry Importer. It brings in camera poses, image references, and points so the model can be checked against the original views. Use undistorted images with their matching camera calibration when projecting textures.
5. Build the room from planes and boxes. The agent can generate Blender Python scripts and render comparison views. Open3D can help identify candidate planes and fit boxes around selected point groups. Object selection and interpretation still need the agent or user; a box-fitting function does not identify a table by itself.
6. If the sparse reference is insufficient, use CPU-capable OpenMVS reconstruction to obtain a denser point cloud or rough mesh. Use it as a modeling reference. It does not have to become the final asset.
7. Review the simple geometry before texturing. Correct wall placement, room dimensions, important openings, and furniture boxes. Keep geometry, source cameras, and images aligned. Any scale or orientation change must be applied consistently to the reconstruction used for texturing.
8. Texture the approved mesh with OpenMVS. Its `TextureMesh` tool accepts camera/image data and a supplied mesh. Choose texture resolution and inspect seams, missing areas, and incorrect projections.
9. Inspect the textured result in Blender, prepare unlit materials if the photographed lighting should stay fixed, and export `.glb`. Keep the editable `.blend` and reconstruction files for later changes.
10. Import into Lince. Add and adjust separate collision Sands once the visual-only mesh and collision-authoring workflow exists.

**How flat photo textures become a room:**

Each surface receives image pixels from photographs that see it clearly. A UV map records where those pixels belong on the mesh. A texture atlas is an image containing the surface patches, like an unfolded cardboard box. After texturing, the original video and capture cameras are not needed to display the asset.

Walls, cabinets, and other nearly flat or box-shaped surfaces suit this well. A table represented as a solid box has sides where the real table had empty space. Those sides need a deliberate appearance: projected imagery, a simple material, or manual cleanup. Surfaces never filmed cannot receive authentic texture from the video. Image generation could fill them, but that would invent appearance.

**What the agent could automate:**

| Task | Agent and tool work | User review |
| --- | --- | --- |
| Prepare footage | Extract and filter frames; run reconstruction | Reshoot missing coverage if necessary |
| Create simple geometry | Fit candidate planes and boxes; write Blender scripts | Decide which objects matter and correct shapes |
| Check placement | Render through reconstructed cameras and compare with photographs | Confirm that the approximation is acceptable |
| Texture and export | Run projection, inspect coverage, configure materials, export GLB | Correct visible seams and choose texture detail |

Prefer an editable list of named shapes with positions, rotations, and sizes as an intermediate output from the agent. A Blender script can turn that list into geometry. This makes requests such as “make the desk one box” repeatable without rebuilding everything manually. This shape description would be a custom part of our workflow, not an existing universal format.

**Files to preserve:**

| File or data | Purpose |
| --- | --- |
| Original video and selected frames | Source appearance and the ability to change frame selection |
| COLMAP reconstruction folder | Camera calibration, poses, image observations, and spatial points |
| Optional point cloud or reference mesh | Helps place the simplified surfaces; this PLY need not contain Gaussians |
| Shape description, scripts, and `.blend` | Editable model and repeatable processing |
| Texture images and final `.glb` | Finished visual asset for Lince |
| Collision definitions | Separate editable bodies sharing the room's placement |

A point cloud alone is not the complete intermediate format: keep the camera reconstruction and original photographs alongside it.

**Tools and hardware choices:**

The laptop inspected during this research has an i7-1165G7, about 16 GB RAM, and Intel integrated graphics. Start with CPU-capable COLMAP alignment and OpenMVS processing, using a small frame set first. Reconstruction cost is separate from the cost of displaying the finished room.

Simple Photogrammetry GUI is an optional interface around these tools and documents a CPU-only Nix package. It is useful for the automatic-reference route; its installation and output still need testing here. Meshroom is another established workflow, especially with NVIDIA hardware. Its standard dense reconstruction requires CUDA; CPU Draft Meshing is not a guarantee of clean simple geometry.

MapAnything's Apache model is worth later experimentation for a spatial reference. Its current demo exports meshes per view with vertex colors, so it still needs consolidation and texture preparation for this goal. Use the Apache model explicitly; the default model weights have a noncommercial license.

**Lince work still needed:**

Lince already imports `.glb` and `.gltf`. Its current mesh-import path generates collision from imported triangles. Add a visual-only mesh option so the room can use manually placed collision Sands instead.

The room and its collision Sands should share a parent placement and be saved, moved, duplicated, and removed together. Collision shapes can remain individually editable inside that group. Hiding their visuals outside Edit mode must leave their physics active. This is proposed behavior, not an implemented feature in this document.

Keep the first test to one room shell and a few large objects. Evaluate texture coverage, placement, import size, and actual Lince performance before expanding to a whole house. Use opaque surfaces, few materials, and modest texture sizes. No performance result is claimed until measured.

**Sources:**

- [COLMAP capture and reconstruction guidance](https://colmap.github.io/tutorial.html) and [reconstruction formats](https://colmap.github.io/format.html).
- [COLMAP CPU and GPU requirements](https://colmap.github.io/faq.html#available-functionality-without-gpu-cuda).
- [Blender Photogrammetry Importer](https://github.com/SBCV/Blender-Addon-Photogrammetry-Importer).
- [Blender Python API](https://docs.blender.org/api/5.1/info_quickstart.html).
- [Open3D plane detection and bounding boxes](https://www.open3d.org/docs/release/tutorial/geometry/pointcloud.html).
- [OpenMVS reconstruction workflow](https://github.com/cdcseacave/openMVS/wiki/Usage) and [2.4.0 texturing options, supplied-mesh input, and GLB export](https://github.com/cdcseacave/openMVS/blob/v2.4.0/apps/TextureMesh/TextureMesh.cpp).
- [fSpy camera matching and Blender import](https://fspy.io/).
- [Simple Photogrammetry GUI and its CPU Nix package](https://github.com/edin45/simple_photogrammetry_gui).
- [Meshroom CUDA limitation](https://meshroom-manual.readthedocs.io/en/latest/faq/needs-cuda/needs-cuda.html) and [texturing replacement geometry](https://meshroom-manual.readthedocs.io/en/latest/faq/texturing-after-retopology/texturing-after-retopology.html).
- [MapAnything model licenses](https://github.com/facebookresearch/map-anything#models) and [mesh export implementation](https://github.com/facebookresearch/map-anything/blob/main/mapanything/utils/hf_utils/viz.py).
- [glTF unlit material specification](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_unlit/README.md).
