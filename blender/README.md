# Blender add-on for importing Teardown saves
Currently supported features:
* Shapes as meshes
  * Lights on voxels not quite supported
* Lights, to some extent
* Everything else as an empty
* Conversion from material PBR properties to the Principled shader
* Player camera
  * Position works, rotation is wrong.

On Linux, use the following command to import /example-inputs/quicksave.bin into Blender:

```bash
cargo run -p teardown-blender-import-installer -- run optional-existing-file.blend
```

Make sure you are using Blender ≥2.91 and that it is built with a recent version of Python. The default built-in Python contains a bug when importing binary libraries.
