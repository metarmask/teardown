import sys
import os
# Ensure imports will always search the folder this script is in first
# Adapted from https://stackoverflow.com/a/16985066
SCRIPT_DIR = os.path.dirname(os.path.realpath(os.path.join(os.getcwd(), os.path.expanduser(__file__))))
sys.path[0] = os.path.normpath(SCRIPT_DIR)

import teardown_import
import bpy
teardown_import.register()
bpy.ops.teardown_import.op_import(filepath=os.path.join(os.path.dirname(__file__), "../../example-input/quicksave.bin"))
