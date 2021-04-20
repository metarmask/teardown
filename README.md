**Tools for parsing and converting the binary format for the Teardown game.**
![The graphical converter interface](https://user-images.githubusercontent.com/7348146/112862805-710c2100-90b6-11eb-8b4f-26c28c606711.png)

## Components
* [Parsing of the binary format](bin-format)
* [User interface to many of the functions of this repository](user-interface)
* [Library for importing saves into the editor](editor-format)
* [Blender add-on for importing saves](blender)

## Converting and opening a level in the editor
1. Download the executable from [the latest release](https://github.com/metarmask/teardown/releases/latest).
2. Run it.
3. Click one of the levels on the left.
4. Beside the text "Convert to..." at the bottom, click the "Editor" button.
5. (Optional) Remove "not-" prefixes from the main.xml file using Find and replace.
6. Open the mod "converted" which should have appeared in Teardown.

Converting levels will reuse the same Vox files in order to save storage and time. The Vox files are stored in "Teardown/data/vox/hash".

## Contributing
Contributions welcome. One way to contribute is to figure out what fields starting with `z_` in this [this file](bin-format/src/format.rs) mean.

Use Rust Nightly. To run the graphical interface, use:

    cargo run --release --package teardown-converter

## Known issues
* Rotation fails for shapes which are rotated on three axes.
* The relative position of vehicles do not work properly.
* The attributes of the following entity kinds are not added to the editor XML:
  * Wheel
  * Joint (not rope joints, though)
  * Location
  * Screen
  * Trigger
* Wheels are added with a "not-" prefix in the XML, removing them from the editor. This is done to prevent crashes.
* Shapes which cannot fit in a MagicaVoxel object are truncated.
