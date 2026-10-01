# Room from video

Research and recommendation: September 23, 2026; expanded October 1, 2026. This covers both a lightweight photo-textured room and a detailed Gaussian splat. These workflows have not yet been tested on the room video. Commands are examples for installed tools, not completed processing.

The goal is a lightweight room made from flat surfaces and simple boxes covered with photographs. A table may be one solid box. Small details belong in the textures. Gaussian splatting remains available in Lince, but is optional for this workflow.

Prepare the visual detail before import. For the mesh route, Lince receives a finished `.glb`; no reconstruction, LLM, or changing geometry detail is required while using the canvas. Collision can come from separate box Sands, visible for editing and hidden during normal use.

**Recommended approach: camera reconstruction, simple Blender geometry, then photo texturing.**

Use COLMAP to establish the room's spatial reference. A vision-capable agent can then help build planes and boxes in Blender, using that reference and rendered previews. OpenMVS projects the photographs onto the final shapes. The user reviews the room and decides what to simplify.

An LLM can draft a room from video alone, but its estimated sizes and positions need checking. Reconstruction tools provide measured image correspondences and camera estimates. The agent can automate Blender through Python; connecting these tools into a reliable review loop is work we would still need to implement.

| Route | Best use | Tradeoff |
| --- | --- | --- |
| COLMAP → simple Blender model → OpenMVS texturing | Preferred for deliberately crude walls and furniture boxes | Needs shape placement and review; sparse points may not describe every surface |
| COLMAP → OpenMVS reconstruction → simplify or replace geometry → texture | Best when an automatic surface is useful as a reference | More processing and cleanup; automatic simplification does not understand which objects should become boxes |
| Selected photographs → fSpy camera matching → manual Blender model and projection | Small scenes, a few key views, or reconstruction failures | More manual alignment and texture work; fSpy matches individual photographs rather than reconstructing a complete video |

**Choose the result and machine:**

| Result | Tools | Finished asset | Main tradeoff |
| --- | --- | --- | --- |
| Simple photo-textured room | FFmpeg, COLMAP, Blender, OpenMVS | GLB | Deliberate shape placement and texture cleanup; intended to be inexpensive to display |
| Detailed Gaussian room | Nerfstudio video processing, COLMAP, Splatfacto | Gaussian PLY | GPU training, larger storage and potentially higher display cost |
| Detailed textured mesh | COLMAP, OpenMVS dense reconstruction, Blender | GLB | More processing, surface cleanup, and more triangles |

The simple room is like a cardboard model covered with photographs. A Gaussian splat uses many colored, partly transparent blobs in space. A detailed mesh follows surfaces with many triangles. Gaussian PLY, ordinary point-cloud PLY, and GLB mesh files contain different kinds of data.

The same video can supply all routes. Preserve selected photographs and the complete camera reconstruction for reuse. The simple room does not require training a Gaussian splat first. No route can recover authentic appearance from surfaces never filmed.

For the mesh routes, install FFmpeg, COLMAP, Blender, Blender Photogrammetry Importer, and OpenMVS from their official distributions. Open3D is optional. Use CPU-capable COLMAP and OpenMVS on the Intel laptop. Confirm command names and options with each installed tool's help and record tool versions.

For Gaussian training, follow the [Nerfstudio installation guide](https://docs.nerf.studio/quickstart/installation.html) in a separate Python environment, with compatible NVIDIA drivers, PyTorch/CUDA, and gsplat. This documented CUDA route needs an NVIDIA machine or rented GPU; the Intel integrated GPU is not suitable. Copy the complete dataset to the training machine and preserve outputs before deleting a rented machine.

The [Splatfacto documentation](https://docs.nerf.studio/nerfology/methods/splat.html) estimates approximately 6 GB GPU memory for `splatfacto` and 12 GB for `splatfacto-big`. These are estimates rather than guaranteed limits. Input resolution and scene size affect memory; leave headroom. GPU memory and computer RAM are separate resources.

**Prepare an existing video:**

1. Copy the original file into a dedicated project folder as `room.mp4`. Keep it unchanged; avoid a compressed messaging-app copy.
2. Watch for blur, focus or exposure changes, moving objects, and missing coverage. Start with one continuous room section.
3. Extract an initial frame set. Two frames per second is a trial value: increase it when neighboring views have insufficient overlap, and remove duplicates when movement is very slow.

```bash
mkdir -p frames sparse
ffmpeg -i room.mp4 -vf "fps=2" -q:v 2 frames/frame_%06d.jpg
```

4. Inspect photographs at full size and remove blurred frames from the working folder. Keep overlapping views from different positions, including corners and furniture edges. Do not independently crop or resize individual images.
5. Record a measured distance for scale. Ordinary video reconstruction does not automatically know metres.
6. Choose one alignment method below. Both methods need inspection before modeling or training.

**Align cameras with COLMAP for mesh creation:**

In COLMAP's GUI, create a project using `frames`, extract features, run sequential matching, and start reconstruction. Disable GPU extraction and matching on the Intel laptop. Use shared calibration for images from one unchanged lens and video configuration; split recordings that switch lenses or zoom.

The basic command-line stages are:

```bash
colmap feature_extractor --database_path database.db --image_path frames --ImageReader.single_camera 1
colmap sequential_matcher --database_path database.db
colmap mapper --database_path database.db --image_path frames --output_path sparse
```

Add your installed release's CPU switches when running without a supported GPU. Their names vary by release; check `colmap feature_extractor -h` and `colmap sequential_matcher -h`. See the [COLMAP command-line guide](https://colmap.github.io/cli.html).

Inspect the reconstruction: cameras should follow the filmed path, important views should register, and furniture should occupy consistent positions. Do not combine disconnected models as though they already share a room coordinate system. Correct failed alignment before proceeding.

Prepare matching undistorted photographs and calibration for OpenMVS. This assumes the accepted reconstruction is `sparse/0`; use its actual folder if different:

```bash
colmap image_undistorter --image_path frames --input_path sparse/0 --output_path undistorted --output_type COLMAP
InterfaceCOLMAP -i undistorted -o scene.mvs
```

Keep the resulting images and camera files together. Follow the installed OpenMVS importer's requirements if its expected layout or model format differs, and inspect the imported scene. The [OpenMVS usage guide](https://github.com/cdcseacave/openMVS/wiki/Usage) covers conversion and downstream processing.

**Alternative preparation through Nerfstudio:**

On the configured Nerfstudio machine, process the video directly:

```bash
ns-process-data video --data room.mp4 --output-dir room-data
```

Or process the already selected photographs instead:

```bash
ns-process-data images --data frames --output-dir room-data
```

Run one of these alternatives. Nerfstudio invokes camera reconstruction; inspect its images and alignment before training. Adjust frame count and processing settings through the installed command's help. The [processing documentation](https://docs.nerf.studio/reference/cli/ns_process_data.html) lists COLMAP and FFmpeg dependencies.

Preserve the complete `room-data` directory. Its COLMAP reconstruction and matching images can also feed Blender/OpenMVS: locate their actual folders and apply the import/undistortion stages above. Nerfstudio can transform coordinates for training, so do not assume an exported splat shares the original reconstruction's orientation or scale without checking.

**The lightweight mesh workflow, step by step:**

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

**Finish the lightweight mesh:**

After reviewing the shapes in Blender, export only the visual mesh as `simple-room.ply`, triangulated and aligned with `scene.mvs`. Importers can change axis conventions; undo any such coordinate conversion on export as necessary and inspect alignment before projection. Keep cameras, points, and mesh in the same coordinate system. Applying room scale after texturing avoids changing only one part of the projection reference.

```bash
TextureMesh scene.mvs --mesh-file simple-room.ply
```

Inspect the output filenames reported by OpenMVS. Import the textured mesh, materials, and images into Blender. Correct seams and missing patches, check visibility from inside the room, configure unlit materials if retaining photographed lighting, and export `simple-room.glb` with textures included. Dense reconstruction is unnecessary when the sparse reference was sufficient.

Start with a modest texture budget, such as 2048-pixel atlases for a trial, then increase where needed. Choose the installed tool's texture-size options explicitly; the example command does not set this budget. Count all atlases when evaluating memory.

**Optional detailed mesh or denser reference:**

For automatic surface reconstruction, the conventional OpenMVS stages are:

```bash
DensifyPointCloud scene.mvs
ReconstructMesh scene_dense.mvs
RefineMesh scene_dense_mesh.mvs
TextureMesh scene_dense_mesh_refine.mvs
```

Use filenames actually produced by your release. Refinement is optional and can be costly. For a detailed mesh, clean the result in Blender and export GLB. For the simple room, use this result as reference, replace geometry with planes/boxes, then texture that replacement. Automatic triangle reduction does not decide that a table should become one solid box. If geometry changes after texturing, inspect UVs and retexture or rebake where needed.

**Create a detailed Gaussian splat:**

1. Prepare and inspect `room-data` using one Nerfstudio processing command above. Correct camera alignment before training.
2. Train the ordinary preset to evaluate the footage:

```bash
ns-train splatfacto --data room-data
```

Or choose the larger, quality-oriented preset when GPU memory permits:

```bash
ns-train splatfacto-big --data room-data
```

3. Inspect the training viewer from several positions within the captured area. Check holes, floating blobs, duplicated furniture, and unstable appearance. More Gaussians do not fix missing views or bad alignment. `splatfacto-big` is a higher-quality preset, not a guarantee of maximum possible quality.
4. Preserve the run's checkpoints and `config.yml`. Export using the actual configuration path printed for your chosen run; replace `RUN` and other path components below as necessary:

```bash
ns-export gaussian-splat --load-config outputs/room-data/splatfacto-big/RUN/config.yml --output-dir exports/room-splat
```

5. Inspect the exported Gaussian PLY in a compatible splat viewer. An ordinary point-cloud viewer cannot validate its full appearance. Keep an untouched export before cropping unwanted floaters or changing the asset.
6. Test that exact export in Lince. Check supported Gaussian properties, orientation, scale, import size, and display performance. Existing Gaussian support does not mean every external export has been tested; record any conversion required.

The [Splatfacto guide](https://docs.nerf.studio/nerfology/methods/splat.html) documents reconstruction-based initialization, presets, and export. Begin with those presets and compare results before further tuning. Preserve training files so later improvements do not require starting from the video again.

**Give either visual asset a physical body:**

Both photo-textured meshes and Gaussian splats can use simple collision floors, walls, and furniture boxes. Gaussians describe appearance and do not automatically provide solid bodies. Align collision with the final visual asset using the measured distance. Decide whether a table should have one solid collision box or usable space underneath.

The visual asset and collision shapes should share one parent placement, so movement, rotation, saving, reopening, duplication, and deletion keep them together. Shapes remain editable inside the group; hiding their editing visuals must leave physics active. This grouping and collision workflow still needs Lince implementation and verification. An exported visual asset alone does not complete the combined visual-and-physical object.

**Check the result before expanding:**

| Check | What to inspect or measure |
| --- | --- |
| Appearance | Texture seams or floating blobs, consistent colors, missing areas, intended walking views |
| Dimensions | Measured distance, floor height, openings, and furniture placement |
| Physics | Solid walls, usable openings, intended furniture collision, active physics with editing visuals hidden |
| Group behavior | Visuals and collision stay together when moved, rotated, saved, reopened, duplicated, and removed |
| Resources | File size, import time, memory use, and frame rate on the intended computer |

Compare outputs on the same machine and from similar viewpoints. Creation speed and display speed are different: slow reconstruction can produce an inexpensive GLB. No performance result is established until measured.

**When processing fails:**

- Too few cameras align: reject blurred frames, preserve more overlapping views, check lens/zoom changes, and reshoot missing connections if needed.
- Plain walls lack points: use corners and nearby textured objects, measured dimensions, or the fSpy fallback.
- Photographs project onto incorrect surfaces: check calibration, undistortion, and coordinate alignment before increasing texture size.
- Surfaces are missing: check whether they were filmed; reshoot or choose a simple material for unseen areas.
- Training runs out of GPU memory: use `splatfacto`, lower image resolution consistently, or choose a larger GPU.
- Display is too slow: reduce triangles/materials/textures for meshes, or crop/reduce Gaussians using a compatible tool. Compare appearance and keep the originals.

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
| Nerfstudio dataset, configuration, checkpoints, and Gaussian PLY | Reproducible training and finished Gaussian asset |
| Tool versions, settings, and scale/orientation transforms | Repeat processing and align visual assets with physics |
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

- [FFmpeg command-line documentation](https://ffmpeg.org/ffmpeg.html).
- [COLMAP command-line workflow](https://colmap.github.io/cli.html).
- [Nerfstudio installation](https://docs.nerf.studio/quickstart/installation.html), [video/image processing](https://docs.nerf.studio/reference/cli/ns_process_data.html), and [Splatfacto training and export](https://docs.nerf.studio/nerfology/methods/splat.html).
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
