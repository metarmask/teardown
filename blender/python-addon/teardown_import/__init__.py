bl_info = {
    "name": "Teardown Importer",
    "author": "metarmask",
    "version": (0, 0, 0),
    "blender": (2, 91, 0),
    "location": "File > Import",
    "description": "Import Teardown saves",
    "support": "COMMUNITY",
    "category": "Import-Export"
}

from .libteardown_import import import_as_collection

import bpy
from bpy_extras.io_utils import ExportHelper, ImportHelper
from bpy_extras.object_utils import AddObjectHelper
from bpy.types import Operator, AddonPreferences
from bpy.props import StringProperty
from mathutils import Matrix, Vector, Euler, Color
import bmesh
from nodeitems_utils import NodeItem, register_node_categories, unregister_node_categories
from nodeitems_builtins import ShaderNodeCategory, CompositorNodeCategory

from pathlib import Path
import math
import json
import sys
import re
import os
from typing import Union
import pickle

tau = 2*math.pi
addon_path = Path(__file__).parent

def make_vertex_list_printable(vertex_list):
    return list(map(lambda x: x.co.to_tuple(), vertex_list))

def max_one_vector(v):
    return Vector((sorted((-1, v.x, 1))[1], sorted((-1, v.y, 1))[1], sorted((-1, v.z, 1))[1]))

def signum(n):
    return math.copysign(1, n)

def flip_with(v, w):
    return Vector((signum(v.x*w.x), signum(v.y*w.y), signum(v.z*w.z)))

def replace_if_not_zero(v, w):
    v = v.copy()
    for i, x in enumerate(w):
        if x != 0:
            v[i] = x
    return v

# pub struct Entity<'a> {
#     pub handle: u32,
#     pub tags: Tags<'a>,
#     pub desc: &'a str,
#     pub kind: Kind<'a>,
#     pub children: Vec::<Entity<'a>>,
# }

def get_entity_name(entity):
    s = ""
    if entity["desc"] != "":
        s += entity["desc"] + " "
    s += str(entity["handle"]) + " " + entity["kind"][0]
    return s

# pub struct Light<'a> {
#     pub unknown_starting_byte: u8,
#     pub kind: Kind,
#     pub transform: Transform,
#     pub rgba: Rgba,
#     pub scale: f32,
#     pub reach: f32,
#     pub size: f32,
#     pub unshadowed: f32,
#     pub cone_angle: AngleRadians,
#     pub cone_penumbra: AngleRadians,
#     pub fog_iter: f32,
#     pub fog_scale: f32,
#     pub something_for_area_light: f32,
#     pub what2: [u8; 17],
#     pub float: f32,
#     pub sound: Sound<'a>,
#     pub glare: f32,
# }
# pub enum Kind {
#     Sphere = 1,Cone = 2,Area = 3,}

def gen_chunk_coords(dimensions):
    for z in range(dimensions[2]):
        for y in range(dimensions[1]):
            for x in range(dimensions[0]):
                yield (x, y, z)
    yield (0, 0, 0)


def iter_chunks(seq, size):
    return (seq[pos:pos + size] for pos in range(0, len(seq), size))

def unpack_positions(positions):
    for i in range(0, len(positions), 3):
        yield (positions[i], positions[i+1], positions[i+2])


n_shapes = 0
tot_n_voxels = 0

tot_n_objects = 0
def create_object(entity, collection):
    global tot_n_objects
    tot_n_objects += 1
    print("Creating object n " + str(tot_n_objects))
    (kind_name, data) = entity["kind"]
    obj = None
    obj_data = None
    if kind_name == "Body":
        pass
    elif kind_name == "Light":
        name = get_entity_name(entity) + " " + data["kind"][0].lower()
        light_kind = data["kind"][0]
        if light_kind == "Sphere":
            light = bpy.data.lights.new(name, "POINT")
            rgba = data["rgba"]
            #print(rgba)
            light.color = (rgba["r"], rgba["g"], rgba["b"])
        elif light_kind == "Cone":
            light = bpy.data.lights.new(name, "SPOT")
            angle = data["cone_angle"]["radians"]
            penumbra = data["cone_penumbra"]["radians"]
            #print(data)
            light.spot_size = angle
            light.spot_blend = penumbra / angle
        elif light_kind == "Area":
            light = bpy.data.lights.new(name, "AREA")
        else:
            raise "Unknown light kind " + light_kind
        # light.distance = data["reach"]
        light.energy = 100
        light.shadow_soft_size = data["size"]
        obj_data = light
    elif kind_name == "Shape":
        n_voxels = int(len(data["voxel_positions"]) / 3)
        global tot_n_voxels
        tot_n_voxels += n_voxels
        if len(data["voxel_positions"]) != 0 and not (n_voxels > 1024*8 or n_voxels > 500000):
            
            # global n_shapes
            # n_shapes += 1
            # print(n_shapes)
            # if n_shapes > 2:
            #     return bpy.data.meshes.new(get_entity_name(entity) + " weird mesh")

            # bm = bmesh.new()
            # # trans = Matrix.Translation(Vector((0.5, 0.5, 0.5)))
            # # diag = Matrix.Diagonal((data["size"][0], data["size"][1], data["size"][2], 0.0))
            # # scale = Matrix.Scale(0.1, 4)
            # chunk_coords = gen_chunk_coords(data["size"])
            # i = 0
            # for (n_times, palette_index) in iter_chunks(data["voxel_data"], 2):
            #     for _ in range(n_times+1):
            #         i += 1
            #         try:
            #             coord = next(chunk_coords)
            #         except:
            #             print("Ran out of coordinates...")
            #             print("size: ", data["size"])
            #             return bpy.data.meshes.new(get_entity_name(entity) + " weird mesh")
            #         if palette_index != 0:
            #             bm.verts.new(coord)
            # #bmesh.ops.create_cube(bm, size=1, matrix=scale @ diag @ trans)
            # me = bpy.data.meshes.new(get_entity_name(entity) + " mesh")
            # bm.to_mesh(me)
            # bm.free()
            me = bpy.data.meshes.new(get_entity_name(entity) + " mesh")

            me.vertices.add(n_voxels)
            me.vertices.foreach_set("co", data["voxel_positions"])
            # vertex_layer = me.vertex_layers_int.new(name="material")
            # vertex_layer.data.foreach_set("value", list(range(n_voxels)))
            me.transform(Matrix.Scale(0.1, 4))
            # me.update()
            # color_layer = me.vertex_colors.new(name="material", do_init=False)
            # if len(data["voxel_materials"]) != 0:
            #     for i in range(n_voxels):
            #         material = data["voxel_materials"][i]
            #         color_layer.data[i].color = (material / 256, 0, 0, 0)

            obj = bpy.data.objects.new(get_entity_name(entity), me)
            obj.modifiers.new("Skin", "SKIN")
            for vertex in obj.data.skin_vertices[0].data:
                vertex.radius = (0.05, 0.05)
            # vertex_group = obj.vertex_groups.new(name="material")
            # for i, material in enumerate(data["voxel_materials"]):
            #     vertex_group.add([i], material / 256, "REPLACE")
        else:
            obj_data = bpy.data.meshes.new(get_entity_name(entity) + " weird mesh")

    if obj == None:
        obj = bpy.data.objects.new(get_entity_name(entity), obj_data)
    if entity["transform"]:
        xyz = entity["transform"]["pos"]
        obj.location = (xyz[0], xyz[1], xyz[2])
        xyzw = entity["transform"]["rot"]
        obj.rotation_mode = "QUATERNION"
        obj.rotation_quaternion = (xyzw[3], xyzw[0], xyzw[1], xyzw[2])
        # obj.matrix_local = Matrix.Rotation(math.radians(-90.0), 4, "X") @ Matrix(obj.matrix_world)
    collection.objects.link(obj)
    #is_world_body = entity["kind"][0] == "Body" and len(entity["children"]) > 40
    is_world_body = False
    for child in entity["children"]:
        child_obj = create_object(child, collection)
        if not is_world_body:
            child_obj.parent = obj
    return obj

bpy.types.Object.texture_tile = bpy.props.IntProperty(name="texture_tile", min=0, max=15)
bpy.types.Object.texture_weight = bpy.props.FloatProperty(name="texture_weight", min=0, max=1)

class ImportBlockModel(Operator, ImportHelper):
    bl_idname = "teardown_import.op_import"
    bl_label = "Import save"

    location: bpy.props.FloatVectorProperty(name="location", subtype="TRANSLATION")

    def execute(self, context):
        hmm = os.path.join(os.path.dirname(__file__), "material.blend")
        with bpy.data.libraries.load(hmm, link=True) as (data_from, data_to):
            data_to.node_groups = data_from.node_groups
        filepath = self.filepath
        # pack, resource_type, resource_id = resources.parse(filepath)
        # resources.packs.append(pack)
        # src_model_file = resource_type.get(resource_id)
        template_material_name = "Teardown template"
        old_template_material = bpy.data.materials.get(template_material_name)
        if old_template_material:
            bpy.data.materials.remove(old_template_material)
        material = bpy.data.materials.new(template_material_name)
        material.use_nodes = True
        shader = material.node_tree
        shader.nodes.clear()
        node_group_user = shader.nodes.new("ShaderNodeGroup")
        node_group_user.node_tree = bpy.data.node_groups.get("Teardown")
        node_group_user.location = (-180, 35.72)
        node_group_user.width = 159
        shader_output = shader.nodes.new("ShaderNodeOutputMaterial")
        shader_output.location = (0, 60)
        shader.links.new(node_group_user.outputs[0], shader_output.inputs[0])

        collection = import_as_collection(filepath)
        bpy.context.collection.children.link(collection)
        # with open(filepath, "rb") as f:
        #     print("Loading pickle file... ", end="")
        #     data = pickle.load(f)
        #     print("Loaded.")
            #print(data)
            # collection = bpy.data.collections.new("Teardown import")
            # for entity in data["entities"]:
            #     obj = create_object(entity, collection)
            # bpy.context.collection.children.link(collection)

        bpy.ops.object.select_all(action="DESELECT")
        for obj in collection.objects:
            obj.select_set(True)
        bpy.ops.transform.rotate(value=tau/4, center_override= (0, 0, 0), orient_axis='X', orient_type='GLOBAL', orient_matrix=((1, 0, 0), (0, 1, 0), (0, 0, 1)), orient_matrix_type='GLOBAL', constraint_axis=(True, False, False), mirror=True, use_proportional_edit=False, proportional_edit_falloff='SMOOTH', proportional_size=1, use_proportional_connected=False, use_proportional_projected=False)
        bpy.ops.object.select_all(action="DESELECT")
        # bpy.ops.transform.transform(mode="ROTATION", value=(tau/4, tau/4, tau/4, tau/4), orient_axis="X", orient_type="GLOBAL", center_override=(0, 0, 0))
        # scene.collection.children.link(collection)
        # empty_data = bpy.data.objects.new(str(resource_id), None)
        # empty_data.location = (Vector((0, 1, 0)) + Vector(self.location)).to_tuple()
        # empty_data.rotation_euler = (tau/4, 0, 0)
        # empty_data.scale = (1/16, 1/16, 1/16)
        # empty_data.empty_display_type = "ARROWS"
        # empty_data.empty_display_size = 16
        # for object in collection.objects:
        #     object.parent = empty_data
        # collection.objects.link(empty_data)
        # # bpy.ops.object.select_all(action="DESELECT")
        # empty_data.select_set(True)
        # bpy.context.view_layer.objects.active = empty_data
        return {"FINISHED"}

classes = (
    ImportBlockModel,
)

register_classes, unregister_classes = bpy.utils.register_classes_factory(classes)

def ui_import_model(self, context):
    self.layout.operator(ImportBlockModel.bl_idname, text="Teardown model (.bin)")

def get_all_spaces():
    for workspace in bpy.data.workspaces:
        for screen in workspace.screens:
            for area in screen.areas:
                for space in area.spaces:
                    yield space

def prepare_environment():
    for space in get_all_spaces():
        prepare_space(space)
    prepare_scene(bpy.context.scene)

def prepare_space(space: bpy.types.Space):
    if space.type == "VIEW_3D":
        pass

def prepare_scene(scene: bpy.types.Scene):
    pass


def register():
    register_classes()
    bpy.types.TOPBAR_MT_file_import.append(ui_import_model)

def unregister():
    unregister_classes()
    bpy.types.TOPBAR_MT_file_import.remove(ui_import_model)
