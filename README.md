**Tools for parsing and converting the binary format for the Teardown game.**
![Marina loaded in editor](https://user-images.githubusercontent.com/7348146/109703505-d511f700-7b95-11eb-99ad-538edb4df014.png)

## Known issues
* Rotation fails for shapes which are rotated on three axes.
* The relative position of lights and shapes does not work properly.
* The attributes of the following entity kinds are not added to the editor XML:
  * Wheel
  * Joint
  * Light
  * Location
  * Screen
  * Trigger
  * Water
* Shapes which cannot fit entirely in a MagicaVoxel object are converted to a simple `voxbox` with the same size as the shape.
* Palettes of non-MagicaVoxel shapes are not rearranged to get the correct materials. (The ground becomes glass)

## Components
* [Parsing of the binary format](bin-format)
* [Command line/user interface to many of the functions of this repository](user-interface)
* [Library for importing saves into the editor](editor-format)
* [Blender add-on for importing saves](blender)
